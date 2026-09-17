pub(crate) const MAX_ZONE_IDENTIFIER_BYTES: usize = 16 * 1024;
pub(crate) const MAX_DISPATCH_STRING_CHARS: usize = 32_767;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ZoneClassification {
    LocalMachine,
    LocalIntranet,
    Trusted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ZoneIdentifierError;

pub(crate) fn parse_zone_identifier(
    contents: &[u8],
) -> Result<ZoneClassification, ZoneIdentifierError> {
    if contents.is_empty() || contents.len() > MAX_ZONE_IDENTIFIER_BYTES {
        return Err(ZoneIdentifierError);
    }
    let text = std::str::from_utf8(contents).map_err(|_| ZoneIdentifierError)?;
    let mut saw_section = false;
    let mut zone_id = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            if line != "[ZoneTransfer]" || saw_section {
                return Err(ZoneIdentifierError);
            }
            saw_section = true;
            continue;
        }
        if !saw_section {
            return Err(ZoneIdentifierError);
        }
        let (key, value) = line.split_once('=').ok_or(ZoneIdentifierError)?;
        if key == "ZoneId" {
            if zone_id.is_some() {
                return Err(ZoneIdentifierError);
            }
            zone_id = Some(value.parse::<u8>().map_err(|_| ZoneIdentifierError)?);
        } else if key.is_empty() {
            return Err(ZoneIdentifierError);
        }
    }
    if !saw_section {
        return Err(ZoneIdentifierError);
    }
    match zone_id.ok_or(ZoneIdentifierError)? {
        0 => Ok(ZoneClassification::LocalMachine),
        1 => Ok(ZoneClassification::LocalIntranet),
        2 => Ok(ZoneClassification::Trusted),
        _ => Err(ZoneIdentifierError),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkbookSafetyError;

pub(crate) fn validate_workbook_safety(
    safety: &crate::ooxml::WorkbookSafetyFacts,
) -> Result<(), WorkbookSafetyError> {
    if safety.has_xlm_macro_sheet
        || safety.has_workbook_protection
        || safety.external_link_count != 0
        || safety.connection_count != 0
        || safety.query_table_count != 0
    {
        Err(WorkbookSafetyError)
    } else {
        Ok(())
    }
}

pub(crate) trait ZoneIdentifierReader: Send + Sync {
    fn read_zone_identifier(
        &self,
        source: &crate::cap_fs::AnchoredPath,
    ) -> std::io::Result<Option<Vec<u8>>>;
}

pub(crate) struct AnchoredWorkbookSourcePolicy<R> {
    reader: R,
}

impl<R> AnchoredWorkbookSourcePolicy<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R: ZoneIdentifierReader> crate::platform::excel::WorkbookSourcePolicy
    for AnchoredWorkbookSourcePolicy<R>
{
    fn validate_source(
        &self,
        source: &crate::cap_fs::AnchoredPath,
        safety: &crate::ooxml::WorkbookSafetyFacts,
    ) -> Result<(), crate::print_queue::PrintFailure> {
        validate_workbook_safety(safety).map_err(|_| {
            crate::print_queue::PrintFailure::unsupported("error.print.excelWorkbookUnsafe")
        })?;
        let zone_identifier = self.reader.read_zone_identifier(source).map_err(|_| {
            crate::print_queue::PrintFailure::unsupported("error.print.excelSourceMotwBlocked")
        })?;
        if let Some(contents) = zone_identifier {
            parse_zone_identifier(&contents).map_err(|_| {
                crate::print_queue::PrintFailure::unsupported("error.print.excelSourceMotwBlocked")
            })?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ComValue {
    I32(i32),
    Bool(bool),
    String(String),
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DispatchInvokeKind {
    Method,
    PropertyGet,
    PropertyPut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EncodedVariant {
    I32(i32),
    Bool(bool),
    String(String),
    Error(i32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EncodedDispatchParameters {
    pub(crate) flags: u16,
    pub(crate) arguments: Vec<EncodedVariant>,
    pub(crate) named_dispids: Vec<i32>,
}

pub(crate) fn encode_dispatch_parameters(
    values: &[ComValue],
    kind: DispatchInvokeKind,
) -> EncodedDispatchParameters {
    const DISP_E_PARAMNOTFOUND_VALUE: i32 = i32::from_ne_bytes(0x8002_0004_u32.to_ne_bytes());
    let arguments = values
        .iter()
        .map(|value| match value {
            ComValue::I32(value) => EncodedVariant::I32(*value),
            ComValue::Bool(value) => EncodedVariant::Bool(*value),
            ComValue::String(value) => EncodedVariant::String(value.clone()),
            ComValue::Missing => EncodedVariant::Error(DISP_E_PARAMNOTFOUND_VALUE),
        })
        .collect();
    let (flags, named_dispids) = match kind {
        DispatchInvokeKind::Method => (1, Vec::new()),
        DispatchInvokeKind::PropertyGet => (2, Vec::new()),
        DispatchInvokeKind::PropertyPut => (4, vec![-3]),
    };
    EncodedDispatchParameters {
        flags,
        arguments,
        named_dispids,
    }
}

impl From<i32> for ComValue {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}

impl From<bool> for ComValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ComOperation {
    InitializeSta,
    CreateApplication,
    SetProperty {
        name: &'static str,
        value: ComValue,
        named_property_put: bool,
    },
    OpenWorkbook {
        arguments: Vec<ComValue>,
    },
    ResolveWorksheet {
        arguments: Vec<ComValue>,
    },
    VerifyWorksheet {
        expected_name: String,
        expected_visibility: i32,
    },
    ExportWorksheetPdf {
        arguments: Vec<ComValue>,
    },
    CloseWorkbook {
        arguments: Vec<ComValue>,
    },
    QuitApplication,
    ReleaseObject(ComObject),
    UninitializeSta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ComObject {
    Application,
    Workbooks,
    Workbook,
    Worksheets,
    Worksheet,
}

pub(crate) struct ComPlanRequest {
    pub(crate) workbook_path: String,
    pub(crate) pdf_path: String,
    pub(crate) workbook_index: u32,
    pub(crate) worksheet_name: String,
    pub(crate) worksheet_visibility: crate::ooxml::WorksheetVisibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ComPlanError;

pub(crate) fn validate_dispatch_member(name: &str) -> Result<(), ComPlanError> {
    match name {
        "AutomationSecurity"
        | "EnableEvents"
        | "DisplayAlerts"
        | "AskToUpdateLinks"
        | "Visible"
        | "Workbooks"
        | "Open"
        | "Worksheets"
        | "Item"
        | "Name"
        | "ExportAsFixedFormat"
        | "Close"
        | "Quit" => Ok(()),
        _ => Err(ComPlanError),
    }
}

pub(crate) fn build_com_operation_plan(
    request: &ComPlanRequest,
) -> Result<Vec<ComOperation>, ComPlanError> {
    if request.workbook_index == 0
        || request.workbook_index > 250
        || !is_bounded_dispatch_string(&request.workbook_path)
        || !is_bounded_dispatch_string(&request.pdf_path)
        || !is_bounded_dispatch_string(&request.worksheet_name)
    {
        return Err(ComPlanError);
    }
    Ok(vec![
        ComOperation::InitializeSta,
        ComOperation::CreateApplication,
        set_property("AutomationSecurity", 3.into()),
        set_property("EnableEvents", false.into()),
        set_property("DisplayAlerts", false.into()),
        set_property("AskToUpdateLinks", false.into()),
        set_property("Visible", false.into()),
        ComOperation::OpenWorkbook {
            arguments: vec![
                ComValue::I32(0),
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Missing,
                ComValue::Missing,
                ComValue::Missing,
                ComValue::Bool(true),
                ComValue::String(String::new()),
                ComValue::String(String::new()),
                ComValue::Missing,
                ComValue::Bool(true),
                ComValue::I32(0),
                ComValue::String(request.workbook_path.clone()),
            ],
        },
        ComOperation::ResolveWorksheet {
            arguments: vec![ComValue::I32(request.workbook_index as i32)],
        },
        ComOperation::VerifyWorksheet {
            expected_name: request.worksheet_name.clone(),
            expected_visibility: worksheet_visibility_value(request.worksheet_visibility),
        },
        ComOperation::ExportWorksheetPdf {
            arguments: vec![
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Missing,
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Bool(true),
                ComValue::I32(0),
                ComValue::String(request.pdf_path.clone()),
                ComValue::I32(0),
            ],
        },
        ComOperation::CloseWorkbook {
            arguments: vec![ComValue::Bool(false)],
        },
        ComOperation::QuitApplication,
        ComOperation::ReleaseObject(ComObject::Worksheet),
        ComOperation::ReleaseObject(ComObject::Worksheets),
        ComOperation::ReleaseObject(ComObject::Workbook),
        ComOperation::ReleaseObject(ComObject::Workbooks),
        ComOperation::ReleaseObject(ComObject::Application),
        ComOperation::UninitializeSta,
    ])
}

fn is_bounded_dispatch_string(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\0')
        && value.encode_utf16().count() <= MAX_DISPATCH_STRING_CHARS
}

fn worksheet_visibility_value(visibility: crate::ooxml::WorksheetVisibility) -> i32 {
    match visibility {
        crate::ooxml::WorksheetVisibility::Visible => -1,
        crate::ooxml::WorksheetVisibility::Hidden => 0,
        crate::ooxml::WorksheetVisibility::VeryHidden => 2,
    }
}

fn set_property(name: &'static str, value: ComValue) -> ComOperation {
    ComOperation::SetProperty {
        name,
        value,
        named_property_put: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AnchoredWorkbookSourcePolicy, ComObject, ComOperation, ComPlanRequest, ComValue,
        DispatchInvokeKind, EncodedVariant, MAX_DISPATCH_STRING_CHARS, MAX_ZONE_IDENTIFIER_BYTES,
        ZoneClassification, ZoneIdentifierReader, build_com_operation_plan,
        encode_dispatch_parameters, parse_zone_identifier, set_property, validate_dispatch_member,
        validate_workbook_safety,
    };
    use crate::ooxml::{WorkbookSafetyFacts, WorksheetVisibility};
    use crate::{
        cap_fs::{AnchoredPath, CapabilityRoot},
        platform::excel::WorkbookSourcePolicy,
    };

    #[test]
    fn safe_zone_identifiers_are_classified_without_starting_windows_apis() {
        for (zone_id, expected) in [
            (0, ZoneClassification::LocalMachine),
            (1, ZoneClassification::LocalIntranet),
            (2, ZoneClassification::Trusted),
        ] {
            let contents = format!("[ZoneTransfer]\r\nZoneId={zone_id}\r\n");
            assert_eq!(parse_zone_identifier(contents.as_bytes()), Ok(expected));
        }
    }

    #[test]
    fn unsafe_malformed_duplicate_and_oversized_zone_identifiers_fail_closed() {
        let mut oversized = b"[ZoneTransfer]\r\nZoneId=0\r\n".to_vec();
        oversized.resize(MAX_ZONE_IDENTIFIER_BYTES + 1, b' ');
        let cases = [
            b"ZoneId=0\r\n".as_slice(),
            b"[ZoneTransfer]\r\n".as_slice(),
            b"[ZoneTransfer]\r\nZoneId=0\r\nZoneId=1\r\n".as_slice(),
            b"[ZoneTransfer]\r\nZoneId=3\r\n".as_slice(),
            b"[ZoneTransfer]\r\nZoneId=4\r\n".as_slice(),
            b"[ZoneTransfer]\r\nZoneId=5\r\n".as_slice(),
            b"[ZoneTransfer]\r\nZoneId=trusted\r\n".as_slice(),
            b"[ZoneTransfer]\r\nZoneId=0\r\n\xff".as_slice(),
            oversized.as_slice(),
        ];
        for contents in cases {
            assert!(parse_zone_identifier(contents).is_err());
        }
    }

    #[test]
    fn workbook_safety_allows_vba_but_rejects_every_preopen_hazard() {
        assert!(validate_workbook_safety(&WorkbookSafetyFacts::default()).is_ok());
        assert!(
            validate_workbook_safety(&WorkbookSafetyFacts {
                has_vba_project: true,
                ..WorkbookSafetyFacts::default()
            })
            .is_ok()
        );
        for unsafe_facts in [
            WorkbookSafetyFacts {
                has_xlm_macro_sheet: true,
                ..WorkbookSafetyFacts::default()
            },
            WorkbookSafetyFacts {
                has_workbook_protection: true,
                ..WorkbookSafetyFacts::default()
            },
            WorkbookSafetyFacts {
                external_link_count: 1,
                ..WorkbookSafetyFacts::default()
            },
            WorkbookSafetyFacts {
                connection_count: 1,
                ..WorkbookSafetyFacts::default()
            },
            WorkbookSafetyFacts {
                query_table_count: 1,
                ..WorkbookSafetyFacts::default()
            },
        ] {
            assert!(validate_workbook_safety(&unsafe_facts).is_err());
        }
    }

    #[test]
    fn com_plan_locks_down_excel_in_governing_order_before_open() {
        let plan = build_com_operation_plan(&ComPlanRequest {
            workbook_path: r"C:\private\source.xlsm".to_string(),
            pdf_path: r"C:\private\sheet.pdf".to_string(),
            workbook_index: 2,
            worksheet_name: "MEC".to_string(),
            worksheet_visibility: WorksheetVisibility::Visible,
        })
        .unwrap();
        assert_eq!(
            plan.iter()
                .take_while(|operation| !matches!(operation, ComOperation::OpenWorkbook { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                ComOperation::InitializeSta,
                ComOperation::CreateApplication,
                set_property("AutomationSecurity", 3.into()),
                set_property("EnableEvents", false.into()),
                set_property("DisplayAlerts", false.into()),
                set_property("AskToUpdateLinks", false.into()),
                set_property("Visible", false.into()),
            ]
        );
        for forbidden in ["Calculate", "CalculateFull", "RefreshAll", "UpdateLink"] {
            assert!(validate_dispatch_member(forbidden).is_err());
        }
    }

    #[test]
    fn every_property_set_uses_dispid_propertyput() {
        let plan = build_com_operation_plan(&ComPlanRequest {
            workbook_path: r"C:\private\source.xlsx".to_string(),
            pdf_path: r"C:\private\sheet.pdf".to_string(),
            workbook_index: 1,
            worksheet_name: "Sheet1".to_string(),
            worksheet_visibility: WorksheetVisibility::Visible,
        })
        .unwrap();
        let setters = plan
            .iter()
            .filter(|operation| matches!(operation, ComOperation::SetProperty { .. }))
            .collect::<Vec<_>>();
        assert_eq!(setters.len(), 5);
        assert!(setters.iter().all(|operation| matches!(
            operation,
            ComOperation::SetProperty {
                named_property_put: true,
                ..
            }
        )));
    }

    #[test]
    fn dispatch_encoding_spec_binds_flags_missing_variant_named_dispid_and_argument_order() {
        let encoded = encode_dispatch_parameters(
            &[
                ComValue::Missing,
                ComValue::I32(7),
                ComValue::Bool(true),
                ComValue::String("final".to_string()),
            ],
            DispatchInvokeKind::PropertyPut,
        );
        assert_eq!(encoded.flags, 4);
        assert_eq!(encoded.named_dispids, vec![-3]);
        assert_eq!(
            encoded.arguments,
            vec![
                EncodedVariant::Error(i32::from_ne_bytes(0x8002_0004_u32.to_ne_bytes())),
                EncodedVariant::I32(7),
                EncodedVariant::Bool(true),
                EncodedVariant::String("final".to_string()),
            ]
        );
        assert_eq!(
            encode_dispatch_parameters(&[], DispatchInvokeKind::Method).flags,
            1
        );
        assert_eq!(
            encode_dispatch_parameters(&[], DispatchInvokeKind::PropertyGet).flags,
            2
        );
        assert!(
            encode_dispatch_parameters(&[], DispatchInvokeKind::Method)
                .named_dispids
                .is_empty()
        );
    }

    #[test]
    fn com_plan_encodes_complete_reversed_dispatch_arguments_and_teardown() {
        let workbook_path = r"C:\private\source.xlsm".to_string();
        let pdf_path = r"C:\private\sheet.pdf".to_string();
        let plan = build_com_operation_plan(&ComPlanRequest {
            workbook_path: workbook_path.clone(),
            pdf_path: pdf_path.clone(),
            workbook_index: 2,
            worksheet_name: "MEC".to_string(),
            worksheet_visibility: WorksheetVisibility::VeryHidden,
        })
        .unwrap();

        assert_eq!(
            plan,
            vec![
                ComOperation::InitializeSta,
                ComOperation::CreateApplication,
                set_property("AutomationSecurity", 3.into()),
                set_property("EnableEvents", false.into()),
                set_property("DisplayAlerts", false.into()),
                set_property("AskToUpdateLinks", false.into()),
                set_property("Visible", false.into()),
                ComOperation::OpenWorkbook {
                    arguments: vec![
                        ComValue::I32(0),
                        ComValue::Missing,
                        ComValue::Bool(false),
                        ComValue::Missing,
                        ComValue::Bool(false),
                        ComValue::Missing,
                        ComValue::Missing,
                        ComValue::Missing,
                        ComValue::Bool(true),
                        ComValue::String(String::new()),
                        ComValue::String(String::new()),
                        ComValue::Missing,
                        ComValue::Bool(true),
                        ComValue::I32(0),
                        ComValue::String(workbook_path),
                    ],
                },
                ComOperation::ResolveWorksheet {
                    arguments: vec![ComValue::I32(2)],
                },
                ComOperation::VerifyWorksheet {
                    expected_name: "MEC".to_string(),
                    expected_visibility: 2,
                },
                ComOperation::ExportWorksheetPdf {
                    arguments: vec![
                        ComValue::Missing,
                        ComValue::Bool(false),
                        ComValue::Missing,
                        ComValue::Missing,
                        ComValue::Bool(false),
                        ComValue::Bool(true),
                        ComValue::I32(0),
                        ComValue::String(pdf_path),
                        ComValue::I32(0),
                    ],
                },
                ComOperation::CloseWorkbook {
                    arguments: vec![ComValue::Bool(false)],
                },
                ComOperation::QuitApplication,
                ComOperation::ReleaseObject(ComObject::Worksheet),
                ComOperation::ReleaseObject(ComObject::Worksheets),
                ComOperation::ReleaseObject(ComObject::Workbook),
                ComOperation::ReleaseObject(ComObject::Workbooks),
                ComOperation::ReleaseObject(ComObject::Application),
                ComOperation::UninitializeSta,
            ]
        );
    }

    #[test]
    fn dispatch_member_whitelist_rejects_calculation_refresh_link_repair_and_protected_view() {
        for allowed in [
            "AutomationSecurity",
            "EnableEvents",
            "DisplayAlerts",
            "AskToUpdateLinks",
            "Visible",
            "Workbooks",
            "Open",
            "Worksheets",
            "Item",
            "Name",
            "ExportAsFixedFormat",
            "Close",
            "Quit",
        ] {
            assert!(validate_dispatch_member(allowed).is_ok(), "{allowed}");
        }
        for forbidden in [
            "Calculation",
            "CalculateBeforeSave",
            "Calculate",
            "CalculateFull",
            "CalculateFullRebuild",
            "Refresh",
            "RefreshAll",
            "UpdateLink",
            "Repair",
            "OpenAndRepair",
            "ProtectedView",
            "ProtectedViewWindows",
            "Run",
            "Save",
        ] {
            assert!(validate_dispatch_member(forbidden).is_err(), "{forbidden}");
        }
    }

    #[test]
    fn com_plan_rejects_unbounded_paths_names_and_invalid_worksheet_indexes() {
        let valid = || ComPlanRequest {
            workbook_path: r"C:\private\source.xlsx".to_string(),
            pdf_path: r"C:\private\sheet.pdf".to_string(),
            workbook_index: 1,
            worksheet_name: "MEC".to_string(),
            worksheet_visibility: WorksheetVisibility::Visible,
        };
        let mut cases = Vec::new();
        let mut request = valid();
        request.workbook_path = "w".repeat(MAX_DISPATCH_STRING_CHARS + 1);
        cases.push(request);
        let mut request = valid();
        request.pdf_path = "p".repeat(MAX_DISPATCH_STRING_CHARS + 1);
        cases.push(request);
        let mut request = valid();
        request.worksheet_name = "n".repeat(MAX_DISPATCH_STRING_CHARS + 1);
        cases.push(request);
        let mut request = valid();
        request.worksheet_name.clear();
        cases.push(request);
        let mut request = valid();
        request.workbook_index = 0;
        cases.push(request);
        let mut request = valid();
        request.workbook_index = 251;
        cases.push(request);

        for request in cases {
            assert!(build_com_operation_plan(&request).is_err());
        }
    }

    enum FakeZoneRead {
        Missing,
        Bytes(Vec<u8>),
        Error,
    }

    struct FakeZoneReader(FakeZoneRead);

    impl ZoneIdentifierReader for FakeZoneReader {
        fn read_zone_identifier(&self, _source: &AnchoredPath) -> std::io::Result<Option<Vec<u8>>> {
            match &self.0 {
                FakeZoneRead::Missing => Ok(None),
                FakeZoneRead::Bytes(contents) => Ok(Some(contents.clone())),
                FakeZoneRead::Error => Err(std::io::Error::other("injected ADS read failure")),
            }
        }
    }

    #[test]
    fn anchored_source_policy_allows_only_missing_or_safe_zone_stream_and_safe_facts() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("source.xlsx");
        std::fs::write(&path, b"workbook").unwrap();
        let source = CapabilityRoot::open(workspace.path())
            .unwrap()
            .authorize(&path)
            .unwrap()
            .anchor()
            .unwrap();
        let safety = WorkbookSafetyFacts::default();

        for read in [
            FakeZoneRead::Missing,
            FakeZoneRead::Bytes(b"[ZoneTransfer]\r\nZoneId=0\r\n".to_vec()),
            FakeZoneRead::Bytes(b"[ZoneTransfer]\r\nZoneId=2\r\n".to_vec()),
        ] {
            assert!(
                AnchoredWorkbookSourcePolicy::new(FakeZoneReader(read))
                    .validate_source(&source, &safety)
                    .is_ok()
            );
        }

        for read in [
            FakeZoneRead::Bytes(b"[ZoneTransfer]\r\nZoneId=3\r\n".to_vec()),
            FakeZoneRead::Bytes(b"malformed".to_vec()),
            FakeZoneRead::Error,
        ] {
            let failure = AnchoredWorkbookSourcePolicy::new(FakeZoneReader(read))
                .validate_source(&source, &safety)
                .unwrap_err();
            assert_eq!(failure.message, "error.print.excelSourceMotwBlocked");
        }

        let failure = AnchoredWorkbookSourcePolicy::new(FakeZoneReader(FakeZoneRead::Missing))
            .validate_source(
                &source,
                &WorkbookSafetyFacts {
                    has_workbook_protection: true,
                    ..WorkbookSafetyFacts::default()
                },
            )
            .unwrap_err();
        assert_eq!(failure.message, "error.print.excelWorkbookUnsafe");
    }
}
