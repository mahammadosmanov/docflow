use std::{
    ffi::{OsStr, OsString},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::print_queue::{
    PrintAdapter, PrintFailure, PrintJob, PrintJobSupport, PrintSettings, PrinterInventory,
};

const LP: &str = "/usr/bin/lp";
const LPSTAT: &str = "/usr/bin/lpstat";
const LPOPTIONS: &str = "/usr/bin/lpoptions";
const CANCEL: &str = "/usr/bin/cancel";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const CONTROLLED_FORMATS: &[&str] = &["pdf", "png", "jpg", "jpeg", "bmp", "tif", "tiff"];

#[derive(Default)]
pub struct MacOsPrintAdapter;

impl PrintAdapter for MacOsPrintAdapter {
    fn inventory(&self) -> Result<PrinterInventory, PrintFailure> {
        let printers = run_bounded(LPSTAT, [OsStr::new("-p")], COMMAND_TIMEOUT).ok();
        let default = run_bounded(LPSTAT, [OsStr::new("-d")], COMMAND_TIMEOUT).ok();
        let names = printers
            .as_ref()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter_map(parse_printer_line)
                    .collect()
            })
            .unwrap_or_default();
        let default_printer = default.and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .find_map(parse_default_line)
        });
        Ok(PrinterInventory {
            printers: names,
            default_printer,
        }
        .normalized())
    }

    fn inspect(
        &self,
        path: &Path,
        printer: &str,
        settings: &PrintSettings,
    ) -> Result<PrintJobSupport, PrintFailure> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !CONTROLLED_FORMATS.contains(&extension.as_str()) {
            return Err(PrintFailure::unsupported(
                "macOS CUPS does not accept this document format directly",
            ));
        }
        if !safe_cups_destination(printer) {
            return Err(PrintFailure::failed("printer name is invalid"));
        }
        validate_reported_options(printer, settings)?;
        Ok(PrintJobSupport::controlled())
    }

    fn submit(&self, job: &PrintJob, cancelled: &dyn Fn() -> bool) -> Result<(), PrintFailure> {
        if cancelled() {
            return Err(PrintFailure::cancelled(
                "cancelled before CUPS submission",
                false,
            ));
        }
        let inventory = self.inventory()?;
        if !inventory.contains(&job.printer) {
            return Err(PrintFailure::failed("printer is no longer available"));
        }
        let path = Path::new(&job.path);
        self.inspect(path, &job.printer, &job.settings())?;
        let args = lp_arguments(job);
        let output = run_bounded(LP, args.iter().map(OsString::as_os_str), COMMAND_TIMEOUT)
            .map_err(|error| {
                if error.kind == crate::print_queue::PrintFailureKind::Uncertain {
                    error
                } else {
                    PrintFailure::failed("CUPS rejected the print job")
                }
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let job_id = parse_job_id(&stdout, &job.printer)
            .ok_or_else(|| PrintFailure::uncertain("CUPS accepted a job without a job ID"))?;
        wait_for_job(&job.printer, job_id, job.timeout_minutes, cancelled)
    }
}

fn lp_arguments(job: &PrintJob) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-d"),
        OsString::from(&job.printer),
        OsString::from("-t"),
        OsString::from(format!("DocFlow - {}", display_name(&job.path))),
    ];
    for option in cups_options(&job.settings()) {
        args.push(OsString::from("-o"));
        args.push(OsString::from(option));
    }
    args.push(OsString::from("--"));
    args.push(OsString::from(&job.path));
    args
}

pub fn cups_options(settings: &PrintSettings) -> Vec<String> {
    let mut options = vec!["job-sheets=none".to_string()];
    if settings.paper != "auto" {
        options.push(format!("media={}", settings.paper));
    }
    match settings.orientation.as_str() {
        "portrait" => options.push("orientation-requested=3".to_string()),
        "landscape" => options.push("orientation-requested=4".to_string()),
        _ => {}
    }
    match settings.scaling.as_str() {
        "fit" => options.push("fit-to-page".to_string()),
        "actual" => options.push("print-scaling=none".to_string()),
        "shrink" => options.push("print-scaling=auto-fit".to_string()),
        _ => {}
    }
    match settings.color.as_str() {
        "color" => options.push("print-color-mode=color".to_string()),
        "grayscale" => options.push("print-color-mode=monochrome".to_string()),
        _ => {}
    }
    match settings.duplex.as_str() {
        "simplex" => options.push("sides=one-sided".to_string()),
        "long" => options.push("sides=two-sided-long-edge".to_string()),
        "short" => options.push("sides=two-sided-short-edge".to_string()),
        _ => {}
    }
    options
}

fn validate_reported_options(printer: &str, settings: &PrintSettings) -> Result<(), PrintFailure> {
    let output = run_bounded(
        LPOPTIONS,
        [OsStr::new("-p"), OsStr::new(printer), OsStr::new("-l")],
        COMMAND_TIMEOUT,
    );
    let Ok(output) = output else {
        // Driverless CUPS destinations may not expose legacy PPD options. The
        // standard IPP options are still validated by the scheduler on submit.
        return Ok(());
    };
    let reported = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if reported.trim().is_empty() {
        return Ok(());
    }
    if settings.paper != "auto" && !reported.contains(&settings.paper.to_ascii_lowercase()) {
        return Err(PrintFailure::unsupported_options(
            "the selected paper is not reported by this printer",
        ));
    }
    if settings.duplex != "printer" && !reported.contains("duplex") && !reported.contains("sides") {
        return Err(PrintFailure::unsupported_options(
            "the selected duplex mode is not reported by this printer",
        ));
    }
    if settings.color != "printer"
        && !reported.contains("color")
        && !reported.contains("monochrome")
    {
        return Err(PrintFailure::unsupported_options(
            "the selected color mode is not reported by this printer",
        ));
    }
    Ok(())
}

fn wait_for_job(
    printer: &str,
    job_id: u32,
    timeout_minutes: u32,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PrintFailure> {
    let job_name = format!("{printer}-{job_id}");
    let deadline = Instant::now() + Duration::from_secs(u64::from(timeout_minutes) * 60);
    loop {
        if cancelled() {
            return match run_bounded(CANCEL, [OsStr::new(&job_name)], COMMAND_TIMEOUT) {
                Ok(_) => Err(active_cups_cancellation()),
                Err(_) => Err(PrintFailure::uncertain(
                    "CUPS could not confirm cancellation of the active job",
                )),
            };
        }
        if Instant::now() >= deadline {
            let _ = run_bounded(CANCEL, [OsStr::new(&job_name)], COMMAND_TIMEOUT);
            return Err(PrintFailure::uncertain(
                "CUPS did not finish the print job before its timeout",
            ));
        }
        let output = run_bounded(
            LPSTAT,
            [
                OsStr::new("-W"),
                OsStr::new("not-completed"),
                OsStr::new("-o"),
                OsStr::new(printer),
            ],
            COMMAND_TIMEOUT,
        )
        .map_err(|_| PrintFailure::uncertain("CUPS job state could not be read"))?;
        let active = String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().next() == Some(job_name.as_str()));
        if !active {
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn active_cups_cancellation() -> PrintFailure {
    PrintFailure::cancelled("CUPS accepted cancellation of an active print job", true)
}

fn run_bounded<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a OsStr>,
    timeout: Duration,
) -> Result<Output, PrintFailure> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("LPDEST")
        .env_remove("PRINTER")
        .spawn()
        .map_err(|_| PrintFailure::failed("native CUPS command is unavailable"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|_| PrintFailure::failed("native CUPS command failed"))?;
                if output.status.success() {
                    return Ok(output);
                }
                return Err(PrintFailure::failed("native CUPS command failed"));
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PrintFailure::uncertain("native CUPS command timed out"));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PrintFailure::failed("native CUPS command failed"));
            }
        }
    }
}

fn parse_printer_line(line: &str) -> Option<String> {
    line.strip_prefix("printer ")?
        .split_whitespace()
        .next()
        .filter(|name| safe_cups_destination(name))
        .map(str::to_string)
}

fn parse_default_line(line: &str) -> Option<String> {
    line.strip_prefix("system default destination: ")
        .filter(|name| safe_cups_destination(name))
        .map(str::to_string)
}

fn parse_job_id(output: &str, printer: &str) -> Option<u32> {
    let prefix = format!("request id is {printer}-");
    output
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn safe_cups_destination(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 127
        && name.chars().all(|character| {
            !character.is_control() && !matches!(character, ' ' | '\t' | '/' | '#')
        })
}

fn display_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> PrintSettings {
        PrintSettings {
            paper: "A3".to_string(),
            orientation: "landscape".to_string(),
            scaling: "fit".to_string(),
            color: "grayscale".to_string(),
            duplex: "short".to_string(),
            timeout_minutes: 30,
        }
    }

    #[test]
    fn cups_job_mapping_preserves_paper_orientation_scaling_color_and_duplex() {
        assert_eq!(
            cups_options(&settings()),
            vec![
                "job-sheets=none",
                "media=A3",
                "orientation-requested=4",
                "fit-to-page",
                "print-color-mode=monochrome",
                "sides=two-sided-short-edge",
            ]
        );
    }

    #[test]
    fn cups_parsers_accept_documented_output_and_reject_injection_names() {
        assert_eq!(
            parse_printer_line("printer Test_PDF is idle. enabled since today"),
            Some("Test_PDF".to_string())
        );
        assert_eq!(
            parse_default_line("system default destination: Test_PDF"),
            Some("Test_PDF".to_string())
        );
        assert_eq!(
            parse_job_id("request id is Test_PDF-42 (1 file(s))", "Test_PDF"),
            Some(42)
        );
        assert!(!safe_cups_destination("printer;$(touch /tmp/pwned)"));
        assert!(!safe_cups_destination("printer\nnext"));
    }

    #[test]
    fn accepted_active_cups_cancellation_remains_uncertain() {
        let failure = active_cups_cancellation();
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(failure.uncertain);
    }

    #[test]
    fn isolated_cups_test_printer_is_never_used_without_explicit_opt_in() {
        let Ok(printer) = std::env::var("DOCFLOW_ISOLATED_TEST_PRINTER") else {
            return;
        };
        assert_eq!(
            std::env::var("DOCFLOW_ISOLATED_TEST_PRINTER_ACK").as_deref(),
            Ok("SPOOL_TO_FILE_ONLY"),
            "the integration printer must be explicitly certified as spool-to-file"
        );
        let adapter = MacOsPrintAdapter;
        assert!(
            adapter
                .inventory()
                .expect("CUPS inventory")
                .contains(&printer),
            "the explicitly named isolated printer is unavailable"
        );
    }

    #[test]
    fn missing_or_unconfigured_cups_inventory_is_reviewable() {
        let inventory = MacOsPrintAdapter
            .inventory()
            .expect("missing printers must be represented by an empty inventory");
        assert!(
            inventory
                .printers
                .iter()
                .all(|printer| safe_cups_destination(printer))
        );
    }
}
