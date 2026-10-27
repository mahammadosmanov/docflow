use std::{
    collections::HashSet,
    ffi::OsStr,
    fs::File,
    io::{BufRead, BufReader, Cursor, Seek},
    mem::{size_of, size_of_val},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::GetLastError,
    Graphics::{
        Gdi::{
            BI_RGB, BITMAPINFO, COLORONCOLOR, CreateDCW, DEVMODEW, DIB_RGB_COLORS, DM_COLOR,
            DM_DUPLEX, DM_IN_BUFFER, DM_ORIENTATION, DM_OUT_BUFFER, DM_PAPERSIZE, DMCOLOR_COLOR,
            DMCOLOR_MONOCHROME, DMDUP_HORIZONTAL, DMDUP_SIMPLEX, DMDUP_VERTICAL,
            DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT, DMPAPER_A3, DMPAPER_A4, DMPAPER_A5,
            DMPAPER_LEGAL, DMPAPER_LETTER, DeleteDC, GetDeviceCaps, HALFTONE, HORZRES, LOGPIXELSX,
            LOGPIXELSY, ResetDCW, SetStretchBltMode, StretchDIBits, VERTRES,
        },
        GdiPlus::{
            GdipCreateFromHDC, GdipDeleteGraphics, GdipDisposeImage, GdipDrawImageRectI,
            GdipGetImageHeight, GdipGetImageHorizontalResolution, GdipGetImageVerticalResolution,
            GdipGetImageWidth, GdipImageGetFrameCount, GdipImageGetFrameDimensionsCount,
            GdipImageGetFrameDimensionsList, GdipImageSelectActiveFrame, GdipLoadImageFromFile,
            GdipSetInterpolationMode, GdiplusShutdown, GdiplusStartup, GdiplusStartupInput,
            GpGraphics, GpImage, InterpolationModeHighQualityBicubic, Ok as GDI_PLUS_OK,
        },
        Printing::{
            ClosePrinter, DocumentPropertiesW, EnumJobsW, EnumPrintersW, GetDefaultPrinterW,
            GetJobW, JOB_CONTROL_CANCEL, JOB_INFO_1W, OpenPrinterW, PRINTER_ENUM_CONNECTIONS,
            PRINTER_ENUM_LOCAL, PRINTER_HANDLE, PRINTER_INFO_4W, SetJobW,
        },
    },
    Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage},
    UI::{
        Shell::{ASSOCF_INIT_IGNOREUNKNOWN, ASSOCSTR_COMMAND, AssocQueryStringW, ShellExecuteW},
        WindowsAndMessaging::SW_HIDE,
    },
};

use crate::page_range::{PageRangeCapability, selected_pdf_pages};
use crate::platform::controlled_submit::{
    ControlledSubmitEffects, SubmitSignals, SubmitSnapshot, execute_controlled_submit,
};
use crate::platform::excel::{ControlledPdfSubmitter, ExportedWorksheetPdf};
use crate::platform::{
    WindowsJobQueryError, WindowsPostSubmissionObservation, WindowsSubmittedJobStatus,
    classify_windows_job_query_error, classify_windows_submitted_job_observation,
    consume_controlled_page_bytes, map_controlled_print_failure, select_windows_submitted_job,
    validate_windows_image_frame_count, windows_post_submission_cancellation,
    windows_post_submission_failure, windows_post_submission_observation_error,
};
use crate::print_queue::{
    PrintAdapter, PrintFailure, PrintJob, PrintJobSupport, PrintSettings, PrinterInventory,
};
use crate::progress::CancellationToken;

const PDF_DPI: i32 = 300;
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const HANDLER_JOB_TIMEOUT: Duration = Duration::from_secs(30);
const OBSERVATION_ERROR_GRACE: Duration = Duration::from_secs(3);
const CONTROLLED_IMAGE_FORMATS: &[&str] = &["png", "jpg", "jpeg", "bmp", "tif", "tiff"];

#[derive(Default)]
pub struct WindowsPrintAdapter;

impl PrintAdapter for WindowsPrintAdapter {
    fn inventory(&self) -> Result<PrinterInventory, PrintFailure> {
        Ok(PrinterInventory {
            printers: enumerate_printers()?,
            default_printer: default_printer(),
        }
        .normalized())
    }

    fn inspect(
        &self,
        path: &Path,
        printer: &str,
        settings: &PrintSettings,
    ) -> Result<PrintJobSupport, PrintFailure> {
        let inventory = self.inventory()?;
        if !inventory.contains(printer) {
            return Err(PrintFailure::failed("printer is not available"));
        }
        let extension = extension(path);
        if extension == "pdf" {
            let _ = validated_devmode(printer, settings)?;
            let page_count = crate::windows_pdf::page_count(path).map_err(PrintFailure::failed)?;
            return Ok(PrintJobSupport::controlled_pdf(Some(page_count)));
        }
        if CONTROLLED_IMAGE_FORMATS.contains(&extension.as_str()) {
            let _runtime = GdiPlusRuntime::start()?;
            let image = GdiPlusImage::load(path)?;
            let (_, frame_count) = image.frame_dimension()?;
            validate_windows_image_frame_count(frame_count)?;
            let _ = validated_devmode(printer, settings)?;
            return Ok(PrintJobSupport::single_page());
        }
        if !safe_extension(&extension) {
            return Err(PrintFailure::unsupported(
                "Windows has no approved native print route for this file type",
            ));
        }
        if has_print_handler(&extension, "printto") {
            return Ok(PrintJobSupport::native_application());
        }
        if inventory.default_printer.as_deref() == Some(printer)
            && has_print_handler(&extension, "print")
        {
            return Ok(PrintJobSupport {
                verb: "print".to_string(),
                settings_limited: true,
                page_range: PageRangeCapability::AllOnly,
            });
        }
        Err(PrintFailure::unsupported(
            "the native application required to print this file type is unavailable",
        ))
    }

    fn submit(&self, job: &PrintJob, cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure> {
        if cancelled() {
            return Err(PrintFailure::cancelled(
                "cancelled before Windows print submission",
                false,
            ));
        }
        let path = Path::new(&job.path);
        let support = self.inspect(path, &job.printer, &job.settings())?;
        if support.verb != job.verb {
            return Err(PrintFailure::failed(
                "Windows print capability changed after preview",
            ));
        }
        if job.verb == "docflow" {
            if extension(path) == "pdf" {
                print_pdf(job, cancelled)
            } else {
                print_image(job, cancelled)
            }
        } else {
            print_with_native_application(job, cancelled)
        }
    }
}

#[derive(Default)]
pub(crate) struct WindowsControlledPdfSubmitter;

impl ControlledPdfSubmitter for WindowsControlledPdfSubmitter {
    fn submit_pdf(
        &self,
        job: &PrintJob,
        pdf: &ExportedWorksheetPdf,
        cancellation: CancellationToken,
        deadline: Instant,
    ) -> Result<(), PrintFailure> {
        submit_controlled_pdf(job, pdf.validated_bytes()?, &cancellation, deadline)
    }
}

fn enumerate_printers() -> Result<Vec<String>, PrintFailure> {
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let mut needed = 0;
    let mut returned = 0;
    // SAFETY: a null buffer with size zero only asks EnumPrintersW for the required size; nothing is written.
    unsafe {
        EnumPrintersW(flags, null(), 4, null_mut(), 0, &mut needed, &mut returned);
    }
    if needed == 0 {
        return Ok(Vec::new());
    }
    let mut buffer = AlignedBuffer::new(needed as usize);
    // SAFETY: buffer holds needed bytes, the size reported by the query above, and the count outputs are live locals.
    let ok = unsafe {
        EnumPrintersW(
            flags,
            null(),
            4,
            buffer.as_mut_bytes(),
            needed,
            &mut needed,
            &mut returned,
        )
    };
    if ok == 0 {
        return Err(PrintFailure::failed("Windows printer enumeration failed"));
    }
    // SAFETY: EnumPrintersW succeeded and wrote `returned` PRINTER_INFO_4W records into buffer, which lives for this scope.
    let entries = unsafe {
        std::slice::from_raw_parts(buffer.as_ptr::<PRINTER_INFO_4W>(), returned as usize)
    };
    Ok(entries
        .iter()
        // SAFETY: pPrinterName points into the buffer that the call above filled; wide_ptr_to_string accepts null and NUL-terminated UTF-16.
        .filter_map(|entry| unsafe { wide_ptr_to_string(entry.pPrinterName) })
        .collect())
}

fn default_printer() -> Option<String> {
    let mut length = 0;
    // SAFETY: a null buffer with length zero only asks GetDefaultPrinterW for the required length.
    unsafe {
        GetDefaultPrinterW(null_mut(), &mut length);
    }
    if length == 0 {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize];
    // SAFETY: buffer has length u16 units, the length reported by the query above.
    let ok = unsafe { GetDefaultPrinterW(buffer.as_mut_ptr(), &mut length) };
    if ok == 0 {
        None
    } else {
        wide_buffer_to_string(&buffer)
    }
}

struct PrinterHandle(PRINTER_HANDLE);

impl PrinterHandle {
    fn open(printer: &str) -> Result<Self, PrintFailure> {
        let printer = wide(printer);
        let mut handle = PRINTER_HANDLE::default();
        // SAFETY: printer is a NUL-terminated UTF-16 name that lives across the call, and handle is a live out-parameter.
        if unsafe { OpenPrinterW(printer.as_ptr(), &mut handle, null()) } == 0 {
            return Err(PrintFailure::failed("Windows could not open the printer"));
        }
        Ok(Self(handle))
    }
}

impl Drop for PrinterHandle {
    fn drop(&mut self) {
        // SAFETY: self.0 came from a successful OpenPrinterW and is closed exactly once, here.
        unsafe {
            ClosePrinter(self.0);
        }
    }
}

struct DeviceContext(windows_sys::Win32::Graphics::Gdi::HDC);

impl Drop for DeviceContext {
    fn drop(&mut self) {
        // SAFETY: the context came from CreateDCW and is deleted exactly once, here.
        unsafe {
            DeleteDC(self.0);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RequestedDevMode {
    paper: Option<i16>,
    orientation: Option<i16>,
    color: Option<i16>,
    duplex: Option<i16>,
}

fn requested_devmode(settings: &PrintSettings) -> Result<RequestedDevMode, PrintFailure> {
    let paper = match settings.paper.as_str() {
        "auto" => None,
        "A3" => Some(DMPAPER_A3 as i16),
        "A4" => Some(DMPAPER_A4 as i16),
        "A5" => Some(DMPAPER_A5 as i16),
        "Letter" => Some(DMPAPER_LETTER as i16),
        "Legal" => Some(DMPAPER_LEGAL as i16),
        _ => {
            return Err(PrintFailure::unsupported_options(
                "Windows does not support the selected paper size",
            ));
        }
    };
    let orientation = match settings.orientation.as_str() {
        "auto" => None,
        "portrait" => Some(DMORIENT_PORTRAIT as i16),
        "landscape" => Some(DMORIENT_LANDSCAPE as i16),
        _ => {
            return Err(PrintFailure::unsupported_options(
                "Windows does not support the selected orientation",
            ));
        }
    };
    let color = match settings.color.as_str() {
        "printer" => None,
        "color" => Some(DMCOLOR_COLOR),
        "grayscale" => Some(DMCOLOR_MONOCHROME),
        _ => {
            return Err(PrintFailure::unsupported_options(
                "Windows does not support the selected color mode",
            ));
        }
    };
    let duplex = match settings.duplex.as_str() {
        "printer" => None,
        "simplex" => Some(DMDUP_SIMPLEX),
        "long" => Some(DMDUP_VERTICAL),
        "short" => Some(DMDUP_HORIZONTAL),
        _ => {
            return Err(PrintFailure::unsupported_options(
                "Windows does not support the selected duplex mode",
            ));
        }
    };
    if !matches!(settings.scaling.as_str(), "fit" | "actual" | "shrink") {
        return Err(PrintFailure::unsupported_options(
            "Windows does not support the selected scaling mode",
        ));
    }
    Ok(RequestedDevMode {
        paper,
        orientation,
        color,
        duplex,
    })
}

fn validated_devmode(
    printer: &str,
    settings: &PrintSettings,
) -> Result<AlignedBuffer, PrintFailure> {
    let requested = requested_devmode(settings)?;
    let printer_name = wide(printer);
    let handle = PrinterHandle::open(printer)?;
    // SAFETY: a null output buffer with size zero only asks DocumentPropertiesW for the required size; the handle is open.
    let size = unsafe {
        DocumentPropertiesW(
            null_mut(),
            handle.0,
            printer_name.as_ptr(),
            null_mut(),
            null(),
            0,
        )
    };
    if size < size_of::<DEVMODEW>() as i32 {
        return Err(PrintFailure::unsupported_options(
            "Windows printer settings are unavailable",
        ));
    }
    let mut buffer = AlignedBuffer::new(size as usize);
    let devmode = buffer.as_mut_ptr::<DEVMODEW>();
    // SAFETY: devmode points into buffer, which has size bytes allocated, and DM_OUT_BUFFER asks the driver to fill it.
    if unsafe {
        DocumentPropertiesW(
            null_mut(),
            handle.0,
            printer_name.as_ptr(),
            devmode,
            null(),
            DM_OUT_BUFFER,
        )
    } < 0
    {
        return Err(PrintFailure::unsupported_options(
            "Windows printer defaults could not be read",
        ));
    }
    // SAFETY: devmode is the buffer that the successful query above filled, and it is borrowed exclusively here.
    unsafe {
        apply_requested_devmode(&mut *devmode, requested);
    }
    // SAFETY: devmode is the same filled buffer, passed as input and output, so the driver validates the requested options.
    if unsafe {
        DocumentPropertiesW(
            null_mut(),
            handle.0,
            printer_name.as_ptr(),
            devmode,
            devmode,
            DM_IN_BUFFER | DM_OUT_BUFFER,
        )
    } < 0
    {
        return Err(PrintFailure::unsupported_options(
            "the printer rejected the selected options",
        ));
    }
    // SAFETY: devmode still points into buffer, which lives while buffer is in scope, and was validated above.
    if unsafe { !devmode_matches(&*devmode, requested) } {
        return Err(PrintFailure::unsupported_options(
            "the printer changed an unsupported selected option",
        ));
    }
    Ok(buffer)
}

unsafe fn apply_requested_devmode(devmode: &mut DEVMODEW, requested: RequestedDevMode) {
    if let Some(value) = requested.paper {
        devmode.dmFields |= DM_PAPERSIZE;
        devmode.Anonymous1.Anonymous1.dmPaperSize = value;
    }
    if let Some(value) = requested.orientation {
        devmode.dmFields |= DM_ORIENTATION;
        devmode.Anonymous1.Anonymous1.dmOrientation = value;
    }
    if let Some(value) = requested.color {
        devmode.dmFields |= DM_COLOR;
        devmode.dmColor = value;
    }
    if let Some(value) = requested.duplex {
        devmode.dmFields |= DM_DUPLEX;
        devmode.dmDuplex = value;
    }
}

unsafe fn devmode_matches(devmode: &DEVMODEW, requested: RequestedDevMode) -> bool {
    requested
        .paper
        // SAFETY: reading a DEVMODEW union member is valid because the driver filled the structure the caller passes in.
        .is_none_or(|value| unsafe { devmode.Anonymous1.Anonymous1.dmPaperSize == value })
        && requested
            .orientation
            // SAFETY: reading a DEVMODEW union member is valid because the driver filled the structure the caller passes in.
            .is_none_or(|value| unsafe { devmode.Anonymous1.Anonymous1.dmOrientation == value })
        && requested.color.is_none_or(|value| devmode.dmColor == value)
        && requested
            .duplex
            .is_none_or(|value| devmode.dmDuplex == value)
}

fn print_pdf(job: &PrintJob, cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure> {
    let mut devmode = validated_devmode(&job.printer, &job.settings())?;
    let printer = wide(&job.printer);
    let driver = wide("WINSPOOL");
    // SAFETY: driver and printer are NUL-terminated names that live across the call, and devmode points to a buffer the driver filled.
    let hdc = unsafe {
        CreateDCW(
            driver.as_ptr(),
            printer.as_ptr(),
            null(),
            devmode.as_mut_ptr::<DEVMODEW>(),
        )
    };
    if hdc.is_null() {
        return Err(PrintFailure::failed(
            "Windows could not create the printer device context",
        ));
    }
    let mut context = DeviceContext(hdc);
    let title = wide(&format!("DocFlow - {}", display_name(&job.path)));
    let document = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: title.as_ptr(),
        lpszOutput: null(),
        lpszDatatype: null(),
        fwType: 0,
    };
    // SAFETY: context.0 is a valid device context from CreateDCW, and document is a live DOCINFOW whose title outlives the call.
    let job_id = unsafe { StartDocW(context.0, &document) };
    if job_id <= 0 {
        return Err(PrintFailure::failed("Windows rejected the PDF print job"));
    }
    let deadline = Instant::now() + Duration::from_secs(u64::from(job.timeout_minutes) * 60);
    let result = render_pdf_pages(
        &mut context,
        devmode.as_mut_ptr::<DEVMODEW>(),
        Path::new(&job.path),
        &job.settings(),
        deadline,
        cancelled,
    );
    if let Err(error) = result {
        // SAFETY: context.0 is the device context whose document was started above.
        let _abort_requested = unsafe { AbortDoc(context.0) } > 0;
        return Err(windows_post_submission_failure(error));
    }
    // SAFETY: context.0 is the device context whose document was started above, and EndDoc runs only after a successful print.
    if unsafe { EndDoc(context.0) } <= 0 {
        return Err(PrintFailure::uncertain(
            "Windows could not confirm completion of the PDF spool submission",
        ));
    }
    wait_for_job(&job.printer, job_id as u32, deadline, cancelled)
}

fn submit_controlled_pdf(
    job: &PrintJob,
    pdf_bytes: &[u8],
    cancellation: &CancellationToken,
    deadline: Instant,
) -> Result<(), PrintFailure> {
    submit_controlled_pdf_inner(job, pdf_bytes, cancellation, deadline)
        .map_err(map_controlled_print_failure)
}

struct WindowsControlledSubmitSignals<'a> {
    cancellation: &'a CancellationToken,
    deadline: Instant,
}

impl SubmitSignals for WindowsControlledSubmitSignals<'_> {
    fn sample(&self) -> SubmitSnapshot {
        SubmitSnapshot {
            cancelled: self.cancellation.is_cancelled(),
            deadline_expired: Instant::now() >= self.deadline,
        }
    }
}

struct WindowsControlledSubmitEffects<'a> {
    context: &'a mut DeviceContext,
    devmode: *mut DEVMODEW,
    document_info: &'a DOCINFOW,
    document: &'a crate::windows_pdf::InMemoryPdf,
    pages: &'a [u32],
    settings: &'a PrintSettings,
    printer: &'a str,
    cancellation: &'a CancellationToken,
    deadline: Instant,
}

impl ControlledSubmitEffects for WindowsControlledSubmitEffects<'_> {
    type JobId = u32;

    fn start_doc(&mut self) -> Result<Self::JobId, PrintFailure> {
        // SAFETY: self.context is the device context for this submission, and document_info points to a DOCINFOW that the caller keeps alive for the call.
        let job_id = unsafe { StartDocW(self.context.0, self.document_info) };
        if job_id <= 0 {
            Err(PrintFailure::failed("error.print.excelPrintJobRejected"))
        } else {
            u32::try_from(job_id)
                .map_err(|_| PrintFailure::failed("error.print.excelPrintJobRejected"))
        }
    }

    fn abort_doc(&mut self) -> bool {
        // SAFETY: self.context is the device context whose document start_doc began.
        (unsafe { AbortDoc(self.context.0) }) > 0
    }

    fn render_pages(&mut self) -> Result<(), PrintFailure> {
        render_in_memory_pdf_pages(
            self.context,
            self.devmode,
            InMemoryPdfRenderRequest {
                document: self.document,
                pages: self.pages,
                settings: self.settings,
                deadline: self.deadline,
                cancelled: &|| self.cancellation.is_cancelled(),
            },
        )
    }

    fn end_doc(&mut self) -> Result<(), PrintFailure> {
        // SAFETY: self.context is the device context whose document start_doc began, and EndDoc runs only after the pages were rendered.
        if unsafe { EndDoc(self.context.0) } > 0 {
            Ok(())
        } else {
            Err(PrintFailure::failed(
                "error.print.excelPrintCompletionUnconfirmed",
            ))
        }
    }

    fn observe_job(&mut self, job_id: Self::JobId) -> Result<(), PrintFailure> {
        wait_for_job(self.printer, job_id, self.deadline, &|| {
            self.cancellation.is_cancelled()
        })
    }
}

fn submit_controlled_pdf_inner(
    job: &PrintJob,
    pdf_bytes: &[u8],
    cancellation: &CancellationToken,
    deadline: Instant,
) -> Result<(), PrintFailure> {
    if job.page_range != "all" {
        return Err(PrintFailure::unsupported_options(
            "error.print.worksheetPageRangeAllOnly",
        ));
    }

    let document = crate::windows_pdf::InMemoryPdf::load(pdf_bytes).map_err(|_| {
        PrintFailure::failed("error.print.excelExportMalformedPdf")
            .with_diagnostic("excel_pdf stage=native_load")
    })?;
    let page_count = document.page_count().map_err(|_| {
        PrintFailure::failed("error.print.excelExportMalformedPdf")
            .with_diagnostic("excel_pdf stage=native_page_count")
    })?;
    let pages = selected_pdf_pages("all", page_count)
        .map_err(|error| PrintFailure::failed(error.message))?;
    let mut devmode = validated_devmode(&job.printer, &job.settings())?;
    let printer = wide(&job.printer);
    let driver = wide("WINSPOOL");
    // SAFETY: driver and printer are NUL-terminated names, and devmode points to a buffer that the driver filled and validated.
    let hdc = unsafe {
        CreateDCW(
            driver.as_ptr(),
            printer.as_ptr(),
            null(),
            devmode.as_mut_ptr::<DEVMODEW>(),
        )
    };
    if hdc.is_null() {
        return Err(PrintFailure::failed(
            "error.print.excelPrinterContextUnavailable",
        ));
    }
    let mut context = DeviceContext(hdc);
    let title = wide("DocFlow - Excel worksheet");
    let document_info = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: title.as_ptr(),
        lpszOutput: null(),
        lpszDatatype: null(),
        fwType: 0,
    };
    let signals = WindowsControlledSubmitSignals {
        cancellation,
        deadline,
    };
    let settings = job.settings();
    let mut effects = WindowsControlledSubmitEffects {
        context: &mut context,
        devmode: devmode.as_mut_ptr::<DEVMODEW>(),
        document_info: &document_info,
        document: &document,
        pages: &pages,
        settings: &settings,
        printer: &job.printer,
        cancellation,
        deadline,
    };
    execute_controlled_submit(&signals, &mut effects)
}

struct GdiPlusRuntime(usize);

impl GdiPlusRuntime {
    fn start() -> Result<Self, PrintFailure> {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 1,
        };
        let mut token = 0;
        // SAFETY: input is a live GdiplusStartupInput, and a null optional output pointer is accepted by GdiplusStartup.
        if unsafe { GdiplusStartup(&mut token, &input, null_mut()) } != GDI_PLUS_OK || token == 0 {
            return Err(PrintFailure::unsupported(
                "Windows image codecs are unavailable",
            ));
        }
        Ok(Self(token))
    }
}

impl Drop for GdiPlusRuntime {
    fn drop(&mut self) {
        // SAFETY: self.0 is the token returned by a successful GdiplusStartup, and it is shut down exactly once, in drop.
        unsafe {
            GdiplusShutdown(self.0);
        }
    }
}

struct GdiPlusImage(*mut GpImage);

impl GdiPlusImage {
    fn load(path: &Path) -> Result<Self, PrintFailure> {
        let path = wide_os(path.as_os_str());
        let mut image = null_mut();
        // SAFETY: path is a NUL-terminated UTF-16 string that lives across the call, and image is a live out-pointer; the result is null-checked.
        if unsafe { GdipLoadImageFromFile(path.as_ptr(), &mut image) } != GDI_PLUS_OK
            || image.is_null()
        {
            return Err(PrintFailure::unsupported(
                "Windows could not decode this image",
            ));
        }
        Ok(Self(image))
    }

    fn frame_dimension(&self) -> Result<(Option<windows_sys::core::GUID>, u32), PrintFailure> {
        let mut dimension_count = 0;
        // SAFETY: self.0 is the image that load() created and that this wrapper owns until drop; dimension_count is a live out-parameter.
        if unsafe { GdipImageGetFrameDimensionsCount(self.0, &mut dimension_count) } != GDI_PLUS_OK
        {
            return Err(PrintFailure::failed(
                "Windows could not inspect image frames",
            ));
        }
        if dimension_count == 0 {
            return Ok((None, 1));
        }
        if dimension_count > 16 {
            return Err(PrintFailure::unsupported(
                "Windows image has too many frame dimensions",
            ));
        }
        let mut dimensions = vec![windows_sys::core::GUID::from_u128(0); dimension_count as usize];
        // SAFETY: dimensions has dimension_count GUID slots, the count reported by the query above.
        if unsafe {
            GdipImageGetFrameDimensionsList(self.0, dimensions.as_mut_ptr(), dimension_count)
        } != GDI_PLUS_OK
        {
            return Err(PrintFailure::failed(
                "Windows could not inspect image frame dimensions",
            ));
        }
        let mut selected = None;
        let mut selected_count = 0;
        for dimension in dimensions {
            let mut count = 0;
            // SAFETY: dimension is a GUID from the list above, and count is a live out-parameter.
            if unsafe { GdipImageGetFrameCount(self.0, &dimension, &mut count) } != GDI_PLUS_OK {
                return Err(PrintFailure::failed("Windows could not count image frames"));
            }
            if count > selected_count {
                selected = Some(dimension);
                selected_count = count;
            }
        }
        Ok((selected, selected_count))
    }

    fn select_frame(
        &self,
        dimension: Option<&windows_sys::core::GUID>,
        frame: u32,
    ) -> Result<(), PrintFailure> {
        if let Some(dimension) = dimension
            // SAFETY: self.0 is the image that this wrapper owns, and dimension came from the frame list above.
            && unsafe { GdipImageSelectActiveFrame(self.0, dimension, frame) } != GDI_PLUS_OK
        {
            return Err(PrintFailure::failed(
                "Windows could not select an image frame",
            ));
        }
        Ok(())
    }

    fn metrics(&self) -> Result<(i32, i32, f64, f64), PrintFailure> {
        let mut width = 0;
        let mut height = 0;
        let mut dpi_x = 0.0;
        let mut dpi_y = 0.0;
        // SAFETY: self.0 is a live image owned by this wrapper, and each out-parameter is a live local.
        if unsafe { GdipGetImageWidth(self.0, &mut width) } != GDI_PLUS_OK
            // SAFETY: self.0 is a live image owned by this wrapper, and each out-parameter is a live local.
            || unsafe { GdipGetImageHeight(self.0, &mut height) } != GDI_PLUS_OK
            // SAFETY: self.0 is a live image owned by this wrapper, and each out-parameter is a live local.
            || unsafe { GdipGetImageHorizontalResolution(self.0, &mut dpi_x) } != GDI_PLUS_OK
            // SAFETY: self.0 is a live image owned by this wrapper, and each out-parameter is a live local.
            || unsafe { GdipGetImageVerticalResolution(self.0, &mut dpi_y) } != GDI_PLUS_OK
            || width == 0
            || height == 0
        {
            return Err(PrintFailure::unsupported(
                "Windows image has invalid dimensions",
            ));
        }
        let width = i32::try_from(width)
            .map_err(|_| PrintFailure::unsupported("Windows image is too wide"))?;
        let height = i32::try_from(height)
            .map_err(|_| PrintFailure::unsupported("Windows image is too tall"))?;
        let dpi_x = if dpi_x.is_finite() && dpi_x >= 1.0 {
            f64::from(dpi_x)
        } else {
            96.0
        };
        let dpi_y = if dpi_y.is_finite() && dpi_y >= 1.0 {
            f64::from(dpi_y)
        } else {
            96.0
        };
        Ok((width, height, dpi_x, dpi_y))
    }
}

impl Drop for GdiPlusImage {
    fn drop(&mut self) {
        // SAFETY: self.0 is the image that load() created, and it is disposed exactly once, in drop.
        unsafe {
            GdipDisposeImage(self.0);
        }
    }
}

fn print_image(job: &PrintJob, cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure> {
    let _runtime = GdiPlusRuntime::start()?;
    let image = GdiPlusImage::load(Path::new(&job.path))?;
    let (frame_dimension, frame_count) = image.frame_dimension()?;
    validate_windows_image_frame_count(frame_count)?;
    if cancelled() {
        return Err(PrintFailure::cancelled(
            "cancelled before Windows image print submission",
            false,
        ));
    }
    let mut devmode = validated_devmode(&job.printer, &job.settings())?;
    let printer = wide(&job.printer);
    let driver = wide("WINSPOOL");
    // SAFETY: driver and printer are NUL-terminated names, and devmode points to a buffer that the driver filled and validated.
    let hdc = unsafe {
        CreateDCW(
            driver.as_ptr(),
            printer.as_ptr(),
            null(),
            devmode.as_mut_ptr::<DEVMODEW>(),
        )
    };
    if hdc.is_null() {
        return Err(PrintFailure::failed(
            "Windows could not create the image printer device context",
        ));
    }
    let mut context = DeviceContext(hdc);
    let title = wide(&format!("DocFlow - {}", display_name(&job.path)));
    let document = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: title.as_ptr(),
        lpszOutput: null(),
        lpszDatatype: null(),
        fwType: 0,
    };
    // SAFETY: context is the device context from CreateDCW, and document is a live DOCINFOW whose title outlives the call.
    let job_id = unsafe { StartDocW(context.0, &document) };
    if job_id <= 0 {
        return Err(PrintFailure::failed("Windows rejected the image print job"));
    }
    let deadline = Instant::now() + Duration::from_secs(u64::from(job.timeout_minutes) * 60);
    let result = draw_image_frames(
        &mut context,
        devmode.as_mut_ptr::<DEVMODEW>(),
        &image,
        frame_dimension.as_ref(),
        frame_count,
        &job.settings(),
        deadline,
        cancelled,
    );
    if let Err(error) = result {
        // SAFETY: context is the device context whose document was started above.
        let _abort_requested = unsafe { AbortDoc(context.0) } > 0;
        return Err(windows_post_submission_failure(error));
    }
    // SAFETY: context is the device context whose document was started above, and EndDoc runs only after all frames were drawn.
    if unsafe { EndDoc(context.0) } <= 0 {
        return Err(PrintFailure::uncertain(
            "Windows could not confirm completion of the image spool submission",
        ));
    }
    wait_for_job(&job.printer, job_id as u32, deadline, cancelled)
}

#[allow(clippy::too_many_arguments)]
fn draw_image_frames(
    context: &mut DeviceContext,
    devmode: *mut DEVMODEW,
    image: &GdiPlusImage,
    dimension: Option<&windows_sys::core::GUID>,
    frames: u32,
    settings: &PrintSettings,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PrintFailure> {
    for frame in 0..frames {
        if cancelled() {
            return Err(PrintFailure::cancelled(
                "Windows image job was cancelled while rendering",
                false,
            ));
        }
        if Instant::now() >= deadline {
            return Err(PrintFailure::failed(
                "Windows image rendering exceeded the print timeout",
            ));
        }
        image.select_frame(dimension, frame)?;
        let (width, height, image_dpi_x, image_dpi_y) = image.metrics()?;
        reset_auto_orientation(context, devmode, settings, width, height)?;
        // SAFETY: context.0 is a live device context, and the index constant is a valid GetDeviceCaps argument.
        let printable_width = unsafe { GetDeviceCaps(context.0, HORZRES as i32) };
        // SAFETY: context.0 is a live device context, and the index constant is a valid GetDeviceCaps argument.
        let printable_height = unsafe { GetDeviceCaps(context.0, VERTRES as i32) };
        // SAFETY: context.0 is a live device context, and the index constant is a valid GetDeviceCaps argument.
        let printer_dpi_x = unsafe { GetDeviceCaps(context.0, LOGPIXELSX as i32) };
        // SAFETY: context.0 is a live device context, and the index constant is a valid GetDeviceCaps argument.
        let printer_dpi_y = unsafe { GetDeviceCaps(context.0, LOGPIXELSY as i32) };
        let (destination_width, destination_height) = image_destination(
            width,
            height,
            image_dpi_x,
            image_dpi_y,
            printable_width,
            printable_height,
            printer_dpi_x,
            printer_dpi_y,
            &settings.scaling,
        )?;
        let x = (printable_width - destination_width) / 2;
        let y = (printable_height - destination_height) / 2;
        // SAFETY: context.0 is a live device context, and StartPage is paired with EndPage below.
        if unsafe { StartPage(context.0) } <= 0 {
            return Err(PrintFailure::failed(
                "Windows could not start an image page",
            ));
        }
        let mut graphics: *mut GpGraphics = null_mut();
        // SAFETY: context.0 is a live device context, and graphics is a live out-pointer that is null-checked.
        let drawn = if unsafe { GdipCreateFromHDC(context.0, &mut graphics) } != GDI_PLUS_OK
            || graphics.is_null()
        {
            false
        } else {
            // SAFETY: graphics was created above and is non-null, so this call only changes its interpolation mode.
            let interpolation =
                unsafe { GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic) }
                    == GDI_PLUS_OK;
            let result = interpolation
                // SAFETY: graphics and image.0 are live objects, and the destination rectangle is computed from the printable area above.
                && unsafe {
                    GdipDrawImageRectI(
                        graphics,
                        image.0,
                        x,
                        y,
                        destination_width,
                        destination_height,
                    )
                } == GDI_PLUS_OK;
            // SAFETY: graphics was created above and is deleted exactly once, here.
            unsafe {
                GdipDeleteGraphics(graphics);
            }
            result
        };
        // SAFETY: context.0 is the device context whose page StartPage began above.
        let ended = unsafe { EndPage(context.0) } > 0;
        if !drawn || !ended {
            return Err(PrintFailure::failed("Windows could not draw an image page"));
        }
    }
    Ok(())
}

fn render_pdf_pages(
    context: &mut DeviceContext,
    devmode: *mut DEVMODEW,
    path: &Path,
    settings: &PrintSettings,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PrintFailure> {
    let page_count = crate::windows_pdf::page_count(path).map_err(PrintFailure::failed)?;
    let pages = selected_pdf_pages(&settings.page_range, page_count)
        .map_err(|error| PrintFailure::failed(error.message))?;
    let temporary = tempfile::Builder::new()
        .prefix("docflow-print-")
        .tempdir()
        .map_err(|_| PrintFailure::failed("Windows print workspace is unavailable"))?;
    for page in pages {
        if cancelled() {
            return Err(PrintFailure::cancelled(
                "Windows PDF job was cancelled while rendering",
                false,
            ));
        }
        if Instant::now() >= deadline {
            return Err(PrintFailure::failed(
                "Windows PDF rendering exceeded the print timeout",
            ));
        }
        let image_path = temporary.path().join(format!("page-{page}.png"));
        crate::windows_pdf::render_page_png(path, &image_path, PDF_DPI, page)
            .map_err(PrintFailure::failed)?;
        let bitmap = decode_png(&image_path)?;
        reset_auto_orientation(context, devmode, settings, bitmap.width, bitmap.height)?;
        // SAFETY: context.0 is a live device context, and StartPage is paired with EndPage below.
        if unsafe { StartPage(context.0) } <= 0 {
            return Err(PrintFailure::failed("Windows could not start a PDF page"));
        }
        let printed = draw_bitmap(context.0, &bitmap, &settings.scaling);
        // SAFETY: context.0 is the device context whose page StartPage began above.
        let ended = unsafe { EndPage(context.0) } > 0;
        printed?;
        if !ended {
            return Err(PrintFailure::failed("Windows could not finish a PDF page"));
        }
    }
    Ok(())
}

struct InMemoryPdfRenderRequest<'a> {
    document: &'a crate::windows_pdf::InMemoryPdf,
    pages: &'a [u32],
    settings: &'a PrintSettings,
    deadline: Instant,
    cancelled: &'a dyn Fn() -> bool,
}

fn render_in_memory_pdf_pages(
    context: &mut DeviceContext,
    devmode: *mut DEVMODEW,
    request: InMemoryPdfRenderRequest<'_>,
) -> Result<(), PrintFailure> {
    for &page in request.pages {
        if (request.cancelled)() {
            return Err(PrintFailure::cancelled(
                "Windows PDF job was cancelled while rendering",
                false,
            ));
        }
        if Instant::now() >= request.deadline {
            return Err(PrintFailure::failed(
                "Windows PDF rendering exceeded the print timeout",
            ));
        }
        let bitmap = consume_controlled_page_bytes(
            || {
                request
                    .document
                    .render_page_png_bytes(PDF_DPI, page)
                    .map_err(PrintFailure::failed)
            },
            decode_png_bytes,
        )?;
        reset_auto_orientation(
            context,
            devmode,
            request.settings,
            bitmap.width,
            bitmap.height,
        )?;
        // SAFETY: context.0 is a live device context, and StartPage is paired with EndPage below.
        if unsafe { StartPage(context.0) } <= 0 {
            return Err(PrintFailure::failed("Windows could not start a PDF page"));
        }
        let printed = draw_bitmap(context.0, &bitmap, &request.settings.scaling);
        // SAFETY: context.0 is the device context whose page StartPage began above.
        let ended = unsafe { EndPage(context.0) } > 0;
        printed?;
        if !ended {
            return Err(PrintFailure::failed("Windows could not finish a PDF page"));
        }
    }
    Ok(())
}

fn reset_auto_orientation(
    context: &mut DeviceContext,
    devmode: *mut DEVMODEW,
    settings: &PrintSettings,
    width: i32,
    height: i32,
) -> Result<(), PrintFailure> {
    if settings.orientation != "auto" {
        return Ok(());
    }
    let orientation = if width > height {
        DMORIENT_LANDSCAPE as i16
    } else {
        DMORIENT_PORTRAIT as i16
    };
    // SAFETY: devmode points to the driver-filled DEVMODEW buffer that the caller owns, and only its orientation fields change.
    unsafe {
        (*devmode).dmFields |= DM_ORIENTATION;
        (*devmode).Anonymous1.Anonymous1.dmOrientation = orientation;
    }
    // SAFETY: context.0 is a live device context, and devmode is the buffer whose orientation was just updated.
    let reset = unsafe { ResetDCW(context.0, devmode) };
    if reset.is_null() {
        Err(PrintFailure::unsupported_options(
            "the printer rejected automatic page orientation",
        ))
    } else {
        context.0 = reset;
        Ok(())
    }
}

struct Bitmap {
    width: i32,
    height: i32,
    bgr: Vec<u8>,
}

fn decode_png(path: &Path) -> Result<Bitmap, PrintFailure> {
    let file = File::open(path)
        .map_err(|_| PrintFailure::failed("Windows could not read a rendered PDF page"))?;
    decode_png_reader(BufReader::new(file))
}

fn decode_png_bytes(bytes: &[u8]) -> Result<Bitmap, PrintFailure> {
    decode_png_reader(Cursor::new(bytes))
}

fn decode_png_reader(reader: impl BufRead + Seek) -> Result<Bitmap, PrintFailure> {
    let mut decoder = png::Decoder::new(reader);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|_| PrintFailure::failed("Windows rendered an invalid PDF page"))?;
    let size = reader.output_buffer_size();
    let mut source = vec![0; size];
    let info = reader
        .next_frame(&mut source)
        .map_err(|_| PrintFailure::failed("Windows could not decode a PDF page"))?;
    let width = i32::try_from(info.width)
        .map_err(|_| PrintFailure::failed("Windows PDF page is too wide"))?;
    let height = i32::try_from(info.height)
        .map_err(|_| PrintFailure::failed("Windows PDF page is too tall"))?;
    let source = &source[..info.buffer_size()];
    let stride = ((info.width as usize * 3) + 3) & !3;
    let total = stride
        .checked_mul(info.height as usize)
        .ok_or_else(|| PrintFailure::failed("Windows PDF page is too large"))?;
    let mut bgr = vec![0xff; total];
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => {
            return Err(PrintFailure::failed(
                "Windows PDF renderer returned an unsupported image format",
            ));
        }
    };
    for y in 0..info.height as usize {
        for x in 0..info.width as usize {
            let source_index = (y * info.width as usize + x) * channels;
            let target_index = y * stride + x * 3;
            let (red, green, blue) = match channels {
                1 | 2 => {
                    let value = source[source_index];
                    (value, value, value)
                }
                _ => (
                    source[source_index],
                    source[source_index + 1],
                    source[source_index + 2],
                ),
            };
            bgr[target_index] = blue;
            bgr[target_index + 1] = green;
            bgr[target_index + 2] = red;
        }
    }
    Ok(Bitmap { width, height, bgr })
}

fn draw_bitmap(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    bitmap: &Bitmap,
    scaling: &str,
) -> Result<(), PrintFailure> {
    // SAFETY: hdc is a live device context from the caller, and the index constant is a valid GetDeviceCaps argument.
    let printable_width = unsafe { GetDeviceCaps(hdc, HORZRES as i32) };
    // SAFETY: hdc is a live device context from the caller, and the index constant is a valid GetDeviceCaps argument.
    let printable_height = unsafe { GetDeviceCaps(hdc, VERTRES as i32) };
    // SAFETY: hdc is a live device context from the caller, and the index constant is a valid GetDeviceCaps argument.
    let dpi_x = unsafe { GetDeviceCaps(hdc, LOGPIXELSX as i32) };
    // SAFETY: hdc is a live device context from the caller, and the index constant is a valid GetDeviceCaps argument.
    let dpi_y = unsafe { GetDeviceCaps(hdc, LOGPIXELSY as i32) };
    if printable_width <= 0 || printable_height <= 0 {
        return Err(PrintFailure::unsupported_options(
            "Windows printer has no printable page area",
        ));
    }
    let scale = page_scale(
        bitmap.width,
        bitmap.height,
        printable_width,
        printable_height,
        dpi_x,
        dpi_y,
        scaling,
    )?;
    let width = (f64::from(bitmap.width) * scale).round() as i32;
    let height = (f64::from(bitmap.height) * scale).round() as i32;
    let x = (printable_width - width) / 2;
    let y = (printable_height - height) / 2;
    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = size_of_val(&info.bmiHeader) as u32;
    info.bmiHeader.biWidth = bitmap.width;
    info.bmiHeader.biHeight = -bitmap.height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 24;
    info.bmiHeader.biCompression = BI_RGB;
    // SAFETY: hdc is a live device context, and the mode constant is a valid SetStretchBltMode argument.
    unsafe {
        SetStretchBltMode(hdc, if scale < 1.0 { HALFTONE } else { COLORONCOLOR });
    }
    // SAFETY: hdc is a live device context, and bgr holds the pixel rows that info describes.
    let copied = unsafe {
        StretchDIBits(
            hdc,
            x,
            y,
            width,
            height,
            0,
            0,
            bitmap.width,
            bitmap.height,
            bitmap.bgr.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
            windows_sys::Win32::Graphics::Gdi::SRCCOPY,
        )
    };
    if copied <= 0 {
        Err(PrintFailure::failed(
            "Windows could not draw a rendered PDF page",
        ))
    } else {
        Ok(())
    }
}

fn page_scale(
    image_width: i32,
    image_height: i32,
    printable_width: i32,
    printable_height: i32,
    dpi_x: i32,
    dpi_y: i32,
    scaling: &str,
) -> Result<f64, PrintFailure> {
    if image_width <= 0
        || image_height <= 0
        || printable_width <= 0
        || printable_height <= 0
        || dpi_x <= 0
        || dpi_y <= 0
    {
        return Err(PrintFailure::unsupported_options(
            "Windows printer has invalid page metrics",
        ));
    }
    let fit = f64::min(
        f64::from(printable_width) / f64::from(image_width),
        f64::from(printable_height) / f64::from(image_height),
    );
    let actual = f64::min(f64::from(dpi_x), f64::from(dpi_y)) / f64::from(PDF_DPI);
    match scaling {
        "fit" => Ok(fit),
        "shrink" => Ok(fit.min(actual)),
        "actual" => Ok(actual),
        _ => Err(PrintFailure::unsupported_options(
            "Windows does not support the selected scaling mode",
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn image_destination(
    image_width: i32,
    image_height: i32,
    image_dpi_x: f64,
    image_dpi_y: f64,
    printable_width: i32,
    printable_height: i32,
    printer_dpi_x: i32,
    printer_dpi_y: i32,
    scaling: &str,
) -> Result<(i32, i32), PrintFailure> {
    if image_width <= 0
        || image_height <= 0
        || image_dpi_x <= 0.0
        || image_dpi_y <= 0.0
        || printable_width <= 0
        || printable_height <= 0
        || printer_dpi_x <= 0
        || printer_dpi_y <= 0
    {
        return Err(PrintFailure::unsupported_options(
            "Windows printer or image has invalid page metrics",
        ));
    }
    let actual_width = f64::from(image_width) * f64::from(printer_dpi_x) / image_dpi_x;
    let actual_height = f64::from(image_height) * f64::from(printer_dpi_y) / image_dpi_y;
    let fit = f64::min(
        f64::from(printable_width) / actual_width,
        f64::from(printable_height) / actual_height,
    );
    let scale = match scaling {
        "fit" => fit,
        "shrink" => fit.min(1.0),
        "actual" => 1.0,
        _ => {
            return Err(PrintFailure::unsupported_options(
                "Windows does not support the selected scaling mode",
            ));
        }
    };
    Ok((
        (actual_width * scale).round().max(1.0) as i32,
        (actual_height * scale).round().max(1.0) as i32,
    ))
}

fn print_with_native_application(
    job: &PrintJob,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PrintFailure> {
    let deadline = Instant::now() + Duration::from_secs(u64::from(job.timeout_minutes) * 60);
    if job.verb == "print"
        && WindowsPrintAdapter.inventory()?.default_printer.as_deref() != Some(job.printer.as_str())
    {
        return Err(PrintFailure::failed(
            "the default printer changed after preview",
        ));
    }
    let known_jobs = job_ids(&job.printer)?;
    let verb = wide(&job.verb);
    let file = wide_os(Path::new(&job.path).as_os_str());
    let parameters = (job.verb == "printto").then(|| wide(&format!("\"{}\"", job.printer)));
    // SAFETY: verb, file, and parameters are NUL-terminated strings that outlive the call, and a null window handle is allowed.
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            parameters.as_ref().map_or(null(), |value| value.as_ptr()),
            null(),
            SW_HIDE,
        )
    };
    if result as isize <= 32 {
        return Err(PrintFailure::failed(
            "the native Windows application rejected the print request",
        ));
    }
    let acquisition_deadline = std::cmp::min(Instant::now() + HANDLER_JOB_TIMEOUT, deadline);
    let title = display_name(&job.path).to_ascii_lowercase();
    let mut enumeration_error_since = None;
    let job_id = loop {
        if cancelled() {
            return Err(PrintFailure::cancelled(
                "the native application was started but no spool job was identified",
                true,
            ));
        }
        let observed = match jobs(&job.printer) {
            Ok(observed) => {
                enumeration_error_since = None;
                observed
            }
            Err(_) => {
                let first_error = enumeration_error_since.get_or_insert_with(Instant::now);
                if first_error.elapsed() >= OBSERVATION_ERROR_GRACE {
                    return Err(windows_post_submission_observation_error(
                        WindowsPostSubmissionObservation::JobEnumeration,
                    ));
                }
                thread::sleep(POLL_INTERVAL);
                continue;
            }
        };
        let candidates = observed
            .into_iter()
            .map(|candidate| (candidate.id, candidate.document))
            .collect::<Vec<_>>();
        if let Some(job_id) = select_windows_submitted_job(&known_jobs, &candidates, &title) {
            break job_id;
        }
        if Instant::now() >= acquisition_deadline {
            return Err(PrintFailure::submitted_unconfirmed(
                "the native application did not expose an identifiable spool job",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    };
    wait_for_job(&job.printer, job_id, deadline, cancelled)
}

fn has_print_handler(extension: &str, action: &str) -> bool {
    let association = wide(&format!(".{extension}"));
    let verb = wide(action);
    let mut length = 32_768;
    let mut command = vec![0_u16; length as usize];
    // SAFETY: association and verb are NUL-terminated strings, and command has the length passed in.
    (unsafe {
        AssocQueryStringW(
            ASSOCF_INIT_IGNOREUNKNOWN,
            ASSOCSTR_COMMAND,
            association.as_ptr(),
            verb.as_ptr(),
            command.as_mut_ptr(),
            &mut length,
        )
    }) >= 0
        && length > 1
        && wide_buffer_to_string(&command).is_some()
}

#[derive(Clone, Debug)]
struct SpoolJob {
    id: u32,
    document: String,
}

fn job_ids(printer: &str) -> Result<HashSet<u32>, PrintFailure> {
    Ok(jobs(printer)?.into_iter().map(|job| job.id).collect())
}

fn jobs(printer: &str) -> Result<Vec<SpoolJob>, PrintFailure> {
    let handle = PrinterHandle::open(printer)?;
    let mut needed = 0;
    let mut returned = 0;
    // SAFETY: a null buffer with size zero only asks EnumJobsW for the required size; handle is an open printer handle.
    unsafe {
        EnumJobsW(
            handle.0,
            0,
            u32::MAX,
            1,
            null_mut(),
            0,
            &mut needed,
            &mut returned,
        );
    }
    if needed == 0 {
        return Ok(Vec::new());
    }
    let mut buffer = AlignedBuffer::new(needed as usize);
    // SAFETY: buffer holds needed bytes, the size reported by the query above, and the count outputs are live locals.
    if unsafe {
        EnumJobsW(
            handle.0,
            0,
            u32::MAX,
            1,
            buffer.as_mut_bytes(),
            needed,
            &mut needed,
            &mut returned,
        )
    } == 0
    {
        return Err(PrintFailure::failed(
            "Windows print queue could not be enumerated",
        ));
    }
    // SAFETY: EnumJobsW succeeded and wrote returned JOB_INFO_1W records into buffer, which lives for this scope.
    let entries =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr::<JOB_INFO_1W>(), returned as usize) };
    Ok(entries
        .iter()
        .map(|entry| SpoolJob {
            id: entry.JobId,
            // SAFETY: pDocument points into the buffer that the call above filled; wide_ptr_to_string accepts null.
            document: unsafe { wide_ptr_to_string(entry.pDocument) }.unwrap_or_default(),
        })
        .collect())
}

fn wait_for_job(
    printer: &str,
    job_id: u32,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PrintFailure> {
    let observation_started = Instant::now();
    let handle = loop {
        match PrinterHandle::open(printer) {
            Ok(handle) => break handle,
            Err(_) if cancelled() => {
                return Err(windows_post_submission_cancellation(false));
            }
            Err(_) if Instant::now() >= deadline => {
                return Err(PrintFailure::uncertain(
                    "Windows spool job did not finish before its timeout",
                ));
            }
            Err(_) if observation_started.elapsed() < OBSERVATION_ERROR_GRACE => {
                thread::sleep(POLL_INTERVAL);
            }
            Err(_) => {
                return Err(windows_post_submission_observation_error(
                    WindowsPostSubmissionObservation::PrinterOpen,
                ));
            }
        }
    };
    let mut read_error_since = None;
    loop {
        if cancelled() {
            // SAFETY: handle is an open printer handle, and job_id is a job observed in this queue.
            let cancel_requested =
                unsafe { SetJobW(handle.0, job_id, 0, null(), JOB_CONTROL_CANCEL) } != 0;
            return Err(windows_post_submission_cancellation(cancel_requested));
        }
        if Instant::now() >= deadline {
            // SAFETY: handle is an open printer handle, and job_id is a job observed in this queue.
            let _ = unsafe { SetJobW(handle.0, job_id, 0, null(), JOB_CONTROL_CANCEL) };
            return Err(PrintFailure::uncertain(
                "Windows spool job did not finish before its timeout",
            ));
        }
        match job_status(handle.0, job_id) {
            // EndDoc succeeded before this observer starts. Windows printer
            // drivers normally remove a completed job immediately unless the
            // queue is configured to retain it. Absence without an observed
            // error is therefore the normal terminal "released by spooler"
            // state, not a reason to retry or stop subsequent jobs.
            Ok(status) => match classify_windows_submitted_job_observation(status) {
                WindowsSubmittedJobStatus::Complete => return Ok(()),
                WindowsSubmittedJobStatus::Uncertain => {
                    return Err(windows_post_submission_observation_error(
                        WindowsPostSubmissionObservation::StatusFault,
                    ));
                }
                WindowsSubmittedJobStatus::Pending => {
                    read_error_since = None;
                    thread::sleep(POLL_INTERVAL);
                }
            },
            Err(error) => {
                let first_error = read_error_since.get_or_insert_with(Instant::now);
                if first_error.elapsed() >= OBSERVATION_ERROR_GRACE {
                    return Err(error);
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

fn job_status(handle: PRINTER_HANDLE, job_id: u32) -> Result<Option<u32>, PrintFailure> {
    for _ in 0..3 {
        let mut needed = 0;
        // SAFETY: a null buffer with length zero only asks GetJobW for the required length; handle is an open printer handle.
        unsafe {
            GetJobW(handle, job_id, 1, null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            // SAFETY: GetJobW just failed on this thread, so GetLastError returns its error code.
            return match classify_windows_job_query_error(unsafe { GetLastError() }) {
                WindowsJobQueryError::Released => Ok(None),
                WindowsJobQueryError::RetryWithLargerBuffer | WindowsJobQueryError::Uncertain => {
                    Err(windows_post_submission_observation_error(
                        WindowsPostSubmissionObservation::JobRead,
                    ))
                }
            };
        }
        let mut buffer = AlignedBuffer::new(needed as usize);
        // SAFETY: buffer holds needed bytes, the size from the query above, and the output counter is a live local.
        if unsafe {
            GetJobW(
                handle,
                job_id,
                1,
                buffer.as_mut_bytes(),
                needed,
                &mut needed,
            )
        } != 0
        {
            // SAFETY: GetJobW succeeded and wrote a JOB_INFO_1W at the start of buffer, which is at least that large.
            return Ok(Some(unsafe { (*buffer.as_ptr::<JOB_INFO_1W>()).Status }));
        }
        // SAFETY: the buffered GetJobW call just failed on this thread, so GetLastError returns its error code.
        match classify_windows_job_query_error(unsafe { GetLastError() }) {
            // EPSON and other fast-purging drivers can release the record
            // between the sizing call and the buffered GetJob call.
            WindowsJobQueryError::Released => return Ok(None),
            WindowsJobQueryError::RetryWithLargerBuffer => continue,
            WindowsJobQueryError::Uncertain => {
                return Err(windows_post_submission_observation_error(
                    WindowsPostSubmissionObservation::JobRead,
                ));
            }
        }
    }
    Err(windows_post_submission_observation_error(
        WindowsPostSubmissionObservation::JobRead,
    ))
}

struct AlignedBuffer(Vec<usize>);

impl AlignedBuffer {
    fn new(bytes: usize) -> Self {
        Self(vec![0; bytes.div_ceil(size_of::<usize>())])
    }

    fn as_mut_bytes(&mut self) -> *mut u8 {
        self.0.as_mut_ptr().cast()
    }

    fn as_ptr<T>(&self) -> *const T {
        self.0.as_ptr().cast()
    }

    fn as_mut_ptr<T>(&mut self) -> *mut T {
        self.0.as_mut_ptr().cast()
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn safe_extension(extension: &str) -> bool {
    !extension.is_empty()
        && extension.len() <= 16
        && extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .to_string()
}

fn wide(value: &str) -> Vec<u16> {
    wide_os(OsStr::new(value))
}

fn wide_os(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn wide_buffer_to_string(buffer: &[u16]) -> Option<String> {
    let length = buffer.iter().position(|value| *value == 0)?;
    if length == 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&buffer[..length]))
    }
}

unsafe fn wide_ptr_to_string(pointer: *const u16) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let mut length = 0;
    // SAFETY: the caller guarantees pointer is readable up to a NUL terminator, and the loop stops at 32_768 units.
    while length < 32_768 && unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    if length == 0 || length == 32_768 {
        None
    } else {
        // SAFETY: the first length units were read above and stay within the same allocation.
        Some(String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(pointer, length)
        }))
    }
}

#[cfg(test)]
mod tests;
