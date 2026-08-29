use std::{
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::{
    contracts::{AppPrefs, AuthState, Bootstrap},
    fs_ops::FilesystemAuthority,
    ocr::{NativeOcr, image_pdf_from_png, render_first_page_png},
    operations::process_scans::{OcrAdapter, ProcessScansParams, preview},
    paths::{CallerIdentity, GrantAccess, GrantScope},
    plan_store::{BackendOrigin, PlanBinding},
    registry::{RegistryFacts, operation_registry},
    settings::SettingsStore,
    undo::undo_journal,
};

const CODE: &str = "AZE-SUN-INT-QMS-RFI-CIV-0064";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageSmokeReport {
    ok: bool,
    version: String,
    operation_count: usize,
    bootstrap_millis: u128,
    settings_millis: u128,
    first_ocr_millis: Option<u128>,
    operation_millis: Option<u128>,
    ocr_code: Option<String>,
    ocr_source: Option<String>,
    applied_steps: Option<u64>,
    undone_steps: Option<u64>,
}

pub fn maybe_run<I>(arguments: I) -> Option<i32>
where
    I: IntoIterator<Item = String>,
{
    let mut arguments = arguments.into_iter();
    let _executable = arguments.next();
    if arguments.next().as_deref() != Some("--package-smoke") {
        return None;
    }
    let Some(output) = arguments.next() else {
        eprintln!("package smoke requires an output path or '-'");
        return Some(2);
    };
    let remaining = arguments.collect::<Vec<_>>();
    let skip_ocr = remaining.iter().any(|argument| argument == "--skip-ocr");
    let tesseract = remaining
        .windows(2)
        .find(|pair| pair[0] == "--tesseract")
        .map(|pair| Path::new(&pair[1]));
    let result = run(skip_ocr, tesseract);
    let (status, encoded) = match result {
        Ok(report) => (
            0,
            serde_json::to_string_pretty(&report).expect("smoke report serializes"),
        ),
        Err(error) => (
            1,
            serde_json::to_string_pretty(&json!({"ok": false, "error": error}))
                .expect("smoke error serializes"),
        ),
    };
    if output == "-" {
        println!("{encoded}");
    } else if let Err(error) = write_new(Path::new(&output), encoded.as_bytes()) {
        eprintln!("package smoke report write failed: {error}");
        return Some(1);
    }
    Some(status)
}

fn run(skip_ocr: bool, tesseract: Option<&Path>) -> Result<PackageSmokeReport, String> {
    let fixture = tempfile::tempdir().map_err(|error| error.to_string())?;
    let bootstrap_started = Instant::now();
    let (bootstrap, settings_millis) = bootstrap_settings(fixture.path())?;
    let bootstrap_millis = bootstrap_started.elapsed().as_millis();
    let mut report = PackageSmokeReport {
        ok: true,
        version: bootstrap.version,
        operation_count: bootstrap.operations.len(),
        bootstrap_millis,
        settings_millis,
        first_ocr_millis: None,
        operation_millis: None,
        ocr_code: None,
        ocr_source: None,
        applied_steps: None,
        undone_steps: None,
    };
    if !skip_ocr {
        run_ocr_operation_fixture(fixture.path(), &mut report, tesseract)?;
    }
    Ok(report)
}

fn bootstrap_settings(root: &Path) -> Result<(Bootstrap, u128), String> {
    let started = Instant::now();
    let settings = SettingsStore::new(root.join("settings.json"))
        .map_err(|error| format!("settings load: {error}"))?;
    let preferences = AppPrefs {
        locale: Some("az".to_string()),
        theme: "dark".to_string(),
        font: "sans".to_string(),
        font_scale: 1.2,
        compact: true,
    };
    settings
        .save_preferences(preferences.clone())
        .map_err(|error| format!("settings save: {error}"))?;
    let mut values = Map::new();
    values.insert("keep_original".to_string(), Value::Bool(true));
    values.insert("nested".to_string(), json!({"mustNotPersist": true}));
    settings
        .save_params("rfi.process_scans", values)
        .map_err(|error| format!("operation settings save: {error}"))?;
    if settings.preferences() != preferences {
        return Err("settings round trip changed preferences".to_string());
    }
    if settings
        .params_for("rfi.process_scans")
        .contains_key("nested")
    {
        return Err("settings persisted a non-scalar operation value".to_string());
    }
    let bootstrap = Bootstrap {
        protocol: 1,
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth: AuthState {
            signed_in: false,
            allowed: false,
            is_admin: false,
            session: None,
            user: None,
        },
        prefs: settings.preferences(),
        operations: operation_registry(RegistryFacts::default()),
    };
    if bootstrap.operations.len() != 6 {
        return Err("bootstrap did not expose all six operations".to_string());
    }
    serde_json::to_value(&bootstrap).map_err(|error| format!("bootstrap encode: {error}"))?;
    Ok((bootstrap, started.elapsed().as_millis()))
}

fn run_ocr_operation_fixture(
    root: &Path,
    report: &mut PackageSmokeReport,
    tesseract: Option<&Path>,
) -> Result<(), String> {
    let source = root.join("source");
    let destination = root.join("destination");
    let journal = root.join("journal");
    fs::create_dir_all(&source).map_err(|error| error.to_string())?;
    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    let searchable = root.join("searchable.pdf");
    let raster = root.join("first-page.png");
    let scan = source.join("scan.pdf");
    write_text_pdf(
        &searchable,
        &format!("{CODE} REQUEST FOR INFORMATION PACKAGE OCR FIXTURE"),
    )?;
    render_first_page_png(&searchable, &raster, 300)?;
    image_pdf_from_png(&raster, &scan)?;

    let ocr = tesseract.map_or_else(NativeOcr::discover, NativeOcr::with_tesseract);
    let ocr_started = Instant::now();
    let read = ocr
        .read(&scan)
        .map_err(|error| format!("first packaged OCR: {error:?}"))?;
    report.first_ocr_millis = Some(ocr_started.elapsed().as_millis());
    if read.matched.code != CODE || !read.source.starts_with("ocr:") {
        return Err("image-only packaged OCR did not return the canonical code".to_string());
    }
    report.ocr_code = Some(read.matched.code);
    report.ocr_source = Some(read.source);

    let operation_started = Instant::now();
    let plan = preview(
        &ProcessScansParams {
            source: source.clone(),
            destination: destination.clone(),
            organize_into_rfi_folders: false,
            keep_original: true,
        },
        &ocr,
        |_| {},
        || false,
    )
    .map_err(|error| backend_error("process-scans preview", error))?;
    let authority = FilesystemAuthority::new(&journal, Duration::from_secs(300));
    let caller = CallerIdentity::new("package-smoke", "isolated-session");
    let scope = GrantScope::new(
        &caller.owner_id,
        &caller.session_id,
        "package-smoke-request",
        "rfi.process_scans",
    );
    let source_grant = authority
        .issue_grant(&scope, &source, GrantAccess::ReadWrite)
        .map_err(|error| backend_error("source grant", error))?;
    let destination_grant = authority
        .issue_grant(&scope, &destination, GrantAccess::ReadWrite)
        .map_err(|error| backend_error("destination grant", error))?;
    let cached = authority
        .cache_plan(
            BackendOrigin::Rust,
            PlanBinding::new(
                &caller.owner_id,
                &caller.session_id,
                "rfi.process_scans",
                vec![source_grant.grant_id, destination_grant.grant_id],
            ),
            plan,
        )
        .map_err(|error| backend_error("plan cache", error))?;
    let applied = authority
        .apply(&cached.plan_id, &caller)
        .map_err(|error| backend_error("process-scans apply", error))?;
    let undone = undo_journal(authority.journals(), &applied.journal_id, &caller, None)
        .map_err(|error| backend_error("process-scans undo", error))?;
    if !scan.is_file()
        || fs::read_dir(&destination)
            .map_err(|error| error.to_string())?
            .next()
            .is_some()
    {
        return Err("process-scans apply/undo did not restore the isolated tree".to_string());
    }
    report.operation_millis = Some(operation_started.elapsed().as_millis());
    report.applied_steps = Some(applied.applied);
    report.undone_steps = Some(undone.applied);
    Ok(())
}

fn backend_error(context: &str, error: crate::errors::BackendError) -> String {
    format!("{context}: {} ({})", error.code, error.message)
}

fn write_text_pdf(path: &Path, text: &str) -> Result<(), String> {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let stream = format!("BT /F1 18 Tf 36 700 Td ({escaped}) Tj ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    fs::write(path, bytes).map_err(|error| format!("OCR fixture write: {error}"))
}

fn write_new(path: &Path, content: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    output.write_all(content)?;
    output.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_smoke_bootstrap_and_settings_are_isolated() {
        let fixture = tempfile::tempdir().unwrap();
        let (bootstrap, _) = bootstrap_settings(fixture.path()).unwrap();

        assert_eq!(bootstrap.protocol, 1);
        assert_eq!(bootstrap.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(bootstrap.operations.len(), 6);
        assert_eq!(bootstrap.prefs.locale.as_deref(), Some("az"));
    }
}
