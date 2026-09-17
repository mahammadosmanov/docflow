use std::{io::Read, mem::ManuallyDrop, ptr::null_mut, sync::Arc, time::Instant};

use windows::{
    Win32::{
        Foundation::DISP_E_PARAMNOTFOUND,
        System::{
            Com::{
                CLSCTX_LOCAL_SERVER, CLSIDFromProgID, COINIT_APARTMENTTHREADED, CoCreateInstance,
                CoInitializeEx, CoUninitialize, DISPATCH_FLAGS, DISPATCH_METHOD,
                DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, IDispatch,
            },
            Ole::DISPID_PROPERTYPUT,
            Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR, VT_ERROR, VT_I4},
        },
    },
    core::{BSTR, GUID, PCWSTR, w},
};

use crate::{
    cap_fs::AnchoredPath,
    platform::{
        excel::{
            ExcelExportPipeline, ExcelExportRequest, ExportedWorksheetPdf, WorksheetPdfExporter,
        },
        excel_windows_contract::{
            AnchoredWorkbookSourcePolicy, ComObject, ComOperation, ComPlanRequest, ComValue,
            DispatchInvokeKind, EncodedVariant, MAX_ZONE_IDENTIFIER_BYTES, ZoneIdentifierReader,
            build_com_operation_plan, encode_dispatch_parameters, validate_dispatch_member,
            validate_workbook_safety,
        },
    },
    print_queue::PrintFailure,
    progress::CancellationToken,
};

#[derive(Default)]
pub(crate) struct WindowsZoneIdentifierReader;

impl ZoneIdentifierReader for WindowsZoneIdentifierReader {
    fn read_zone_identifier(&self, source: &AnchoredPath) -> std::io::Result<Option<Vec<u8>>> {
        let Some(stream) = source.open_zone_identifier()? else {
            return Ok(None);
        };
        let mut contents = Vec::new();
        stream
            .take((MAX_ZONE_IDENTIFIER_BYTES + 1) as u64)
            .read_to_end(&mut contents)?;
        if contents.len() > MAX_ZONE_IDENTIFIER_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Zone.Identifier exceeds the bounded source policy limit",
            ));
        }
        Ok(Some(contents))
    }
}

#[derive(Default)]
pub(crate) struct WindowsExcelExporter;

impl WorksheetPdfExporter for WindowsExcelExporter {
    fn export_pdf(
        &self,
        request: ExcelExportRequest,
    ) -> Result<ExportedWorksheetPdf, PrintFailure> {
        let ExcelExportRequest {
            staged_workbook,
            safety,
            worksheet,
            deadline,
            cancellation,
            ..
        } = request;
        let staged_workbook = staged_workbook;
        validate_workbook_safety(&safety)
            .map_err(|_| PrintFailure::unsupported("error.print.excelWorkbookUnsafe"))?;
        check_worker_fence(deadline, &cancellation)?;

        let workbook_path = bounded_path(staged_workbook.workbook_path())?;
        let pdf_output_path = staged_workbook.pdf_output_path();
        let pdf_path = bounded_path(&pdf_output_path)?;
        match std::fs::symlink_metadata(&pdf_output_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(PrintFailure::failed(
                    "error.print.excelExportDestinationExists",
                ));
            }
            Err(_) => return Err(PrintFailure::failed("error.print.excelExportFailed")),
        }
        let plan = build_com_operation_plan(&ComPlanRequest {
            workbook_path,
            pdf_path,
            workbook_index: worksheet.workbook_index,
            worksheet_name: worksheet.name,
            worksheet_visibility: worksheet.visibility,
        })
        .map_err(|_| PrintFailure::unsupported("error.print.excelComContractInvalid"))?;

        let mut apartment = None;
        let mut session = ExcelDispatchSession::default();
        let execution = execute_plan(&plan, &mut apartment, &mut session, deadline, &cancellation);
        let teardown = session.cleanup();
        drop(apartment.take());
        if teardown.is_err() {
            staged_workbook.quarantine_after_teardown_failure();
            return Err(PrintFailure::unsupported(
                "error.print.excelExportQuarantined",
            ));
        }
        execution?;
        let exported_pdf = staged_workbook.into_exported_pdf()?;
        check_worker_fence(deadline, &cancellation)?;
        Ok(exported_pdf)
    }
}

pub(crate) fn production_excel_pipeline() -> Arc<ExcelExportPipeline> {
    Arc::new(ExcelExportPipeline::new(
        Arc::new(AnchoredWorkbookSourcePolicy::new(
            WindowsZoneIdentifierReader,
        )),
        Arc::new(WindowsExcelExporter),
        Arc::new(super::printing::WindowsControlledPdfSubmitter),
    ))
}

fn bounded_path(path: &std::path::Path) -> Result<String, PrintFailure> {
    path.to_str()
        .filter(|value| !value.is_empty() && !value.contains('\0'))
        .map(ToOwned::to_owned)
        .ok_or_else(|| PrintFailure::unsupported("error.print.excelComContractInvalid"))
}

fn check_worker_fence(
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<(), PrintFailure> {
    if cancellation.is_cancelled() {
        return Err(PrintFailure::cancelled(
            "error.print.excelExportCancelled",
            false,
        ));
    }
    if Instant::now() >= deadline {
        return Err(PrintFailure::failed("error.print.excelExportTimedOut"));
    }
    Ok(())
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, PrintFailure> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| com_failure_at("initialize_sta", &error))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[derive(Default)]
struct ExcelDispatchSession {
    application: Option<IDispatch>,
    workbooks: Option<IDispatch>,
    workbook: Option<IDispatch>,
    worksheets: Option<IDispatch>,
    worksheet: Option<IDispatch>,
    workbook_closed: bool,
    application_quit: bool,
}

impl ExcelDispatchSession {
    fn cleanup(&mut self) -> Result<(), PrintFailure> {
        let mut failed = false;
        if !self.workbook_closed
            && let Some(workbook) = self.workbook.as_ref()
        {
            let mut arguments = values_to_variants(&[ComValue::Bool(false)]);
            if invoke(workbook, "Close", DISPATCH_METHOD, &mut arguments, false).is_err() {
                failed = true;
            } else {
                self.workbook_closed = true;
            }
        }
        if !self.application_quit
            && let Some(application) = self.application.as_ref()
        {
            if invoke(application, "Quit", DISPATCH_METHOD, &mut [], false).is_err() {
                failed = true;
            } else {
                self.application_quit = true;
            }
        }
        self.worksheet.take();
        self.worksheets.take();
        self.workbook.take();
        self.workbooks.take();
        self.application.take();
        if failed { Err(com_failure()) } else { Ok(()) }
    }

    fn release(&mut self, object: ComObject) -> Result<(), PrintFailure> {
        let released = match object {
            ComObject::Worksheet => self.worksheet.take(),
            ComObject::Worksheets => self.worksheets.take(),
            ComObject::Workbook => self.workbook.take(),
            ComObject::Workbooks => self.workbooks.take(),
            ComObject::Application => self.application.take(),
        };
        if released.is_none() {
            return Err(PrintFailure::unsupported(
                "error.print.excelComContractInvalid",
            ));
        }
        Ok(())
    }
}

impl Drop for ExcelDispatchSession {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn execute_plan(
    plan: &[ComOperation],
    apartment: &mut Option<ComApartment>,
    session: &mut ExcelDispatchSession,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<(), PrintFailure> {
    for operation in plan {
        check_worker_fence(deadline, cancellation)?;
        match operation {
            ComOperation::InitializeSta => {
                if apartment.is_some() {
                    return Err(contract_failure());
                }
                *apartment = Some(ComApartment::initialize()?);
            }
            ComOperation::CreateApplication => {
                if apartment.is_none() || session.application.is_some() {
                    return Err(contract_failure());
                }
                let class = unsafe { CLSIDFromProgID(w!("Excel.Application")) }
                    .map_err(|error| unavailable_at("resolve_excel", &error))?;
                let application: IDispatch =
                    unsafe { CoCreateInstance(&class, None, CLSCTX_LOCAL_SERVER) }
                        .map_err(|error| unavailable_at("create_excel", &error))?;
                session.application = Some(application);
            }
            ComOperation::SetProperty {
                name,
                value,
                named_property_put,
            } => {
                if !named_property_put {
                    return Err(contract_failure());
                }
                let application = session.application.as_ref().ok_or_else(contract_failure)?;
                let mut arguments = values_to_variants(std::slice::from_ref(value));
                invoke(
                    application,
                    name,
                    DISPATCH_PROPERTYPUT,
                    &mut arguments,
                    true,
                )?;
            }
            ComOperation::OpenWorkbook { arguments } => {
                let application = session.application.as_ref().ok_or_else(contract_failure)?;
                let mut no_arguments = [];
                let workbooks = variant_dispatch(invoke(
                    application,
                    "Workbooks",
                    DISPATCH_PROPERTYGET,
                    &mut no_arguments,
                    false,
                )?)?;
                let mut arguments = values_to_variants(arguments);
                let workbook = variant_dispatch(invoke(
                    &workbooks,
                    "Open",
                    DISPATCH_METHOD,
                    &mut arguments,
                    false,
                )?)?;
                session.workbooks = Some(workbooks);
                session.workbook = Some(workbook);
            }
            ComOperation::ResolveWorksheet { arguments } => {
                let workbook = session.workbook.as_ref().ok_or_else(contract_failure)?;
                let mut no_arguments = [];
                let worksheets = variant_dispatch(invoke(
                    workbook,
                    "Worksheets",
                    DISPATCH_PROPERTYGET,
                    &mut no_arguments,
                    false,
                )?)?;
                let mut arguments = values_to_variants(arguments);
                let worksheet = variant_dispatch(invoke(
                    &worksheets,
                    "Item",
                    DISPATCH_PROPERTYGET,
                    &mut arguments,
                    false,
                )?)?;
                session.worksheets = Some(worksheets);
                session.worksheet = Some(worksheet);
            }
            ComOperation::VerifyWorksheet {
                expected_name,
                expected_visibility,
            } => {
                let worksheet = session.worksheet.as_ref().ok_or_else(contract_failure)?;
                let mut no_arguments = [];
                let actual_name = variant_string(invoke(
                    worksheet,
                    "Name",
                    DISPATCH_PROPERTYGET,
                    &mut no_arguments,
                    false,
                )?)?;
                let actual_visibility = variant_i32(invoke(
                    worksheet,
                    "Visible",
                    DISPATCH_PROPERTYGET,
                    &mut no_arguments,
                    false,
                )?)?;
                if actual_name != *expected_name || actual_visibility != *expected_visibility {
                    return Err(PrintFailure::failed("error.print.worksheetChanged"));
                }
            }
            ComOperation::ExportWorksheetPdf { arguments } => {
                let worksheet = session.worksheet.as_ref().ok_or_else(contract_failure)?;
                let mut arguments = values_to_variants(arguments);
                invoke(
                    worksheet,
                    "ExportAsFixedFormat",
                    DISPATCH_METHOD,
                    &mut arguments,
                    false,
                )
                .map_err(|_| PrintFailure::failed("error.print.excelExportFailed"))?;
            }
            ComOperation::CloseWorkbook { arguments } => {
                let workbook = session.workbook.as_ref().ok_or_else(contract_failure)?;
                let mut arguments = values_to_variants(arguments);
                invoke(workbook, "Close", DISPATCH_METHOD, &mut arguments, false)?;
                session.workbook_closed = true;
            }
            ComOperation::QuitApplication => {
                let application = session.application.as_ref().ok_or_else(contract_failure)?;
                invoke(application, "Quit", DISPATCH_METHOD, &mut [], false)?;
                session.application_quit = true;
            }
            ComOperation::ReleaseObject(object) => session.release(*object)?,
            ComOperation::UninitializeSta => {
                if session.application.is_some()
                    || session.workbooks.is_some()
                    || session.workbook.is_some()
                    || session.worksheets.is_some()
                    || session.worksheet.is_some()
                {
                    return Err(contract_failure());
                }
                drop(apartment.take().ok_or_else(contract_failure)?);
            }
        }
    }
    Ok(())
}

fn values_to_variants(values: &[ComValue]) -> Vec<VARIANT> {
    encode_dispatch_parameters(values, DispatchInvokeKind::Method)
        .arguments
        .iter()
        .map(encoded_variant_to_variant)
        .collect()
}

fn encoded_variant_to_variant(value: &EncodedVariant) -> VARIANT {
    match value {
        EncodedVariant::I32(value) => (*value).into(),
        EncodedVariant::Bool(value) => (*value).into(),
        EncodedVariant::String(value) => value.as_str().into(),
        EncodedVariant::Error(code) => VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_ERROR,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { scode: *code },
                }),
            },
        },
    }
}

fn invoke(
    dispatch: &IDispatch,
    name: &str,
    flags: DISPATCH_FLAGS,
    arguments: &mut [VARIANT],
    property_put: bool,
) -> Result<VARIANT, PrintFailure> {
    validate_dispatch_member(name).map_err(|_| contract_failure())?;
    let kind = if flags == DISPATCH_METHOD && !property_put {
        DispatchInvokeKind::Method
    } else if flags == DISPATCH_PROPERTYGET && !property_put {
        DispatchInvokeKind::PropertyGet
    } else if flags == DISPATCH_PROPERTYPUT && property_put {
        DispatchInvokeKind::PropertyPut
    } else {
        return Err(contract_failure());
    };
    let encoded = encode_dispatch_parameters(&[], kind);
    if flags.0 != encoded.flags {
        return Err(contract_failure());
    }
    debug_assert_eq!(
        i32::from_ne_bytes(0x8002_0004_u32.to_ne_bytes()),
        DISP_E_PARAMNOTFOUND.0
    );
    debug_assert_eq!(
        encoded.named_dispids.as_slice(),
        match kind {
            DispatchInvokeKind::PropertyPut => std::slice::from_ref(&DISPID_PROPERTYPUT),
            DispatchInvokeKind::Method | DispatchInvokeKind::PropertyGet => &[],
        }
    );
    let wide = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let name_pointer = PCWSTR(wide.as_ptr());
    let iid_null = GUID::from_u128(0);
    let mut dispatch_id = 0;
    unsafe { dispatch.GetIDsOfNames(&iid_null, &name_pointer, 1, 0, &mut dispatch_id) }
        .map_err(|error| com_failure_at(&format!("lookup:{name}"), &error))?;

    let mut named_dispids = encoded.named_dispids;
    let parameters = DISPPARAMS {
        rgvarg: if arguments.is_empty() {
            null_mut()
        } else {
            arguments.as_mut_ptr()
        },
        rgdispidNamedArgs: if named_dispids.is_empty() {
            null_mut()
        } else {
            named_dispids.as_mut_ptr()
        },
        cArgs: u32::try_from(arguments.len()).map_err(|_| contract_failure())?,
        cNamedArgs: u32::try_from(named_dispids.len()).map_err(|_| contract_failure())?,
    };
    let mut result = VARIANT::default();
    unsafe {
        dispatch.Invoke(
            dispatch_id,
            &iid_null,
            0,
            flags,
            &parameters,
            Some(&mut result),
            None,
            None,
        )
    }
    .map_err(|error| {
        let action = match kind {
            DispatchInvokeKind::Method => "method",
            DispatchInvokeKind::PropertyGet => "get",
            DispatchInvokeKind::PropertyPut => "set",
        };
        com_failure_at(&format!("{action}:{name}"), &error)
    })?;
    Ok(result)
}

fn variant_dispatch(value: VARIANT) -> Result<IDispatch, PrintFailure> {
    IDispatch::try_from(&value).map_err(|_| com_failure())
}

fn variant_string(value: VARIANT) -> Result<String, PrintFailure> {
    if value.vt() != VT_BSTR {
        return Err(com_failure());
    }
    let bstr = unsafe {
        &*(&value.Anonymous.Anonymous.Anonymous.bstrVal as *const ManuallyDrop<BSTR> as *const BSTR)
    };
    String::from_utf16(bstr).map_err(|_| com_failure())
}

fn variant_i32(value: VARIANT) -> Result<i32, PrintFailure> {
    if value.vt() != VT_I4 {
        return Err(com_failure());
    }
    Ok(unsafe { value.Anonymous.Anonymous.Anonymous.lVal })
}

fn contract_failure() -> PrintFailure {
    PrintFailure::unsupported("error.print.excelComContractInvalid")
}

fn com_failure() -> PrintFailure {
    PrintFailure::failed("error.print.excelComFailed")
        .with_diagnostic("excel_com stage=result_decode")
}

fn com_failure_at(stage: &str, error: &windows::core::Error) -> PrintFailure {
    PrintFailure::failed("error.print.excelComFailed").with_diagnostic(format!(
        "excel_com stage={stage} hresult=0x{:08X}",
        error.code().0 as u32
    ))
}

fn unavailable_at(stage: &str, error: &windows::core::Error) -> PrintFailure {
    PrintFailure::unsupported("error.print.excelUnavailable").with_diagnostic(format!(
        "excel_com stage={stage} hresult=0x{:08X}",
        error.code().0 as u32
    ))
}
