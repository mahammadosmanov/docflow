//! Source-authoritative, backup-first publication to two RFI System replicas.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    contracts::{JsonObject, Plan, ProgressEvent, Step, StepKind, Warning},
    errors::BackendError,
    fs_ops::file_sha256,
    parallel::bounded_map_stream,
    rfi_system_sync::DISCIPLINES,
    scan_index::{FileFingerprint, ScanIndex, ScanIndexStats},
};

pub const OPERATION_ID: &str = "files.replica_publish";
const MAX_ENTRIES: usize = 500_000;
const MAX_ROOT_FILES: usize = 64;
const MAX_WARNINGS: usize = 200;
const HASH_PROFILE: &str = "sha256-v1";
const MAX_INTERVAL_MINUTES: u64 = 1_440;
const MAX_SETTLE_SECONDS: u64 = 3_600;
const MAX_CHANGED_FILES: u64 = 500_000;
const MAX_CHANGED_MIB: u64 = 1_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplicaPublishParams {
    pub root_a: PathBuf,
    pub root_b: PathBuf,
    pub root_c: PathBuf,
    pub source_root: String,
    pub backup_root: PathBuf,
    #[serde(default)]
    pub root_files: String,
    #[serde(default)]
    pub unattended: bool,
    #[serde(default = "default_interval_minutes")]
    pub interval_minutes: u64,
    #[serde(default = "default_settle_seconds")]
    pub settle_seconds: u64,
    #[serde(default = "default_max_changed_files")]
    pub max_changed_files: u64,
    #[serde(default = "default_max_changed_mib")]
    pub max_changed_mib: u64,
}

fn default_interval_minutes() -> u64 {
    15
}

fn default_settle_seconds() -> u64 {
    30
}

fn default_max_changed_files() -> u64 {
    250
}

fn default_max_changed_mib() -> u64 {
    2_048
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReplicaStepMeta {
    pub source_sha256: String,
    pub expected_target_sha256: String,
    pub backup_path: String,
    pub relative_path: String,
    pub target_name: String,
}

#[derive(Clone, Debug)]
struct FileSnapshot {
    path: PathBuf,
    hash: String,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Default)]
struct TreeSnapshot {
    files: BTreeMap<PathBuf, FileSnapshot>,
    directories: BTreeSet<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CachedHash {
    sha256: String,
}

pub fn preview<P>(params: &ReplicaPublishParams, mut progress: P) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
{
    preview_inner(params, None, false, &mut progress)
}

pub fn preview_with_index<P>(
    params: &ReplicaPublishParams,
    owner_id: &str,
    index: &ScanIndex,
    mut progress: P,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
{
    preview_inner(params, Some((owner_id, index)), false, &mut progress)
}

pub fn preview_automatic_with_index<P>(
    params: &ReplicaPublishParams,
    owner_id: &str,
    index: &ScanIndex,
    mut progress: P,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
{
    preview_inner(params, Some((owner_id, index)), true, &mut progress)
}

fn source_root_index(params: &ReplicaPublishParams) -> Result<usize, BackendError> {
    match params.source_root.as_str() {
        "a" => Ok(0),
        "b" => Ok(1),
        "c" => Ok(2),
        _ => Err(BackendError::validation("error.replica.sourceInvalid")),
    }
}

fn validate_schedule_limits(params: &ReplicaPublishParams) -> Result<(), BackendError> {
    if !(1..=MAX_INTERVAL_MINUTES).contains(&params.interval_minutes) {
        return Err(BackendError::validation("error.replica.intervalInvalid"));
    }
    if !(5..=MAX_SETTLE_SECONDS).contains(&params.settle_seconds) {
        return Err(BackendError::validation("error.replica.settleInvalid"));
    }
    if !(1..=MAX_CHANGED_FILES).contains(&params.max_changed_files)
        || !(1..=MAX_CHANGED_MIB).contains(&params.max_changed_mib)
    {
        return Err(BackendError::validation("error.replica.changeLimitInvalid"));
    }
    Ok(())
}

fn preview_inner<P>(
    params: &ReplicaPublishParams,
    cache: Option<(&str, &ScanIndex)>,
    automatic: bool,
    progress: &mut P,
) -> Result<Plan, BackendError>
where
    P: FnMut(ProgressEvent),
{
    let roots = canonical_roots(params)?;
    let source_index = source_root_index(params)?;
    let source_root = &roots[source_index];
    let backup_root = canonical_directory(&params.backup_root, "error.replica.backupInvalid")?;
    validate_root_topology(&roots, &backup_root)?;
    let root_files = parse_root_files(&params.root_files)?;
    validate_schedule_limits(params)?;

    progress(ProgressEvent {
        phase: "replica_discovery".to_string(),
        done: 0,
        total: 3,
        subject: source_root.to_string_lossy().into_owned(),
    });
    let mut source = snapshot_tree(source_root, &root_files, true)?;
    if automatic {
        ensure_source_settled(&source, params.settle_seconds)?;
    }
    progress(ProgressEvent {
        phase: "replica_discovery".to_string(),
        done: 1,
        total: 3,
        subject: source_root.to_string_lossy().into_owned(),
    });
    let mut scan_stats = ScanIndexStats::default();
    hash_snapshot(
        source_root,
        &mut source,
        cache,
        &mut scan_stats,
        progress,
        "replica_source_hash",
    )?;

    let run_id = Uuid::new_v4().simple().to_string();
    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    let mut rows = Vec::new();
    let mut target_number = 0_u64;
    let mut changed_bytes = 0_u64;
    for (index, target_root) in roots.iter().enumerate() {
        if index == source_index {
            continue;
        }
        target_number += 1;
        let target_name = ["A", "B", "C"][index];
        let mut target = snapshot_tree(target_root, &root_files, false)?;
        hash_snapshot(
            target_root,
            &mut target,
            cache,
            &mut scan_stats,
            progress,
            "replica_target_hash",
        )?;
        progress(ProgressEvent {
            phase: "replica_discovery".to_string(),
            done: target_number + 1,
            total: 3,
            subject: target_root.to_string_lossy().into_owned(),
        });

        let replacement_paths = source
            .files
            .iter()
            .filter_map(|(relative, source_file)| {
                target
                    .files
                    .get(relative)
                    .filter(|target_file| target_file.hash != source_file.hash)
                    .map(|_| relative.clone())
            })
            .collect::<BTreeSet<_>>();
        let backup_run = backup_root
            .join(&run_id)
            .join(format!("target-{target_name}"));
        let mut directories = source
            .directories
            .iter()
            .map(|relative| target_root.join(relative))
            .filter(|path| !path.exists())
            .collect::<BTreeSet<_>>();
        for relative in &replacement_paths {
            if let Some(parent) = relative.parent() {
                collect_missing_directories(
                    &backup_root,
                    &backup_run.join(parent),
                    &mut directories,
                )?;
            }
        }
        for directory in directories {
            steps.push(Step {
                kind: StepKind::Mkdir,
                src: None,
                dst: Some(directory.to_string_lossy().into_owned()),
                note_key: Some("note.replica.mkdir".to_string()),
                meta: JsonObject::new(),
            });
        }

        for (relative, source_file) in &source.files {
            let destination = target_root.join(relative);
            match target.files.get(relative) {
                None => {
                    changed_bytes = changed_bytes.saturating_add(source_file.len);
                    let mut meta = JsonObject::new();
                    meta.insert(
                        "expectedSourceSha256".to_string(),
                        Value::String(source_file.hash.clone()),
                    );
                    steps.push(Step {
                        kind: StepKind::Copy,
                        src: Some(source_file.path.to_string_lossy().into_owned()),
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: Some("note.replica.copy".to_string()),
                        meta,
                    });
                    rows.push(review_row(
                        target_name,
                        relative,
                        "copy",
                        "",
                        &source_file.hash,
                    ));
                }
                Some(target_file) if target_file.hash == source_file.hash => {}
                Some(target_file) => {
                    changed_bytes = changed_bytes.saturating_add(source_file.len);
                    let backup_path = backup_run.join(relative);
                    let step_meta = ReplicaStepMeta {
                        source_sha256: source_file.hash.clone(),
                        expected_target_sha256: target_file.hash.clone(),
                        backup_path: backup_path.to_string_lossy().into_owned(),
                        relative_path: relative.to_string_lossy().into_owned(),
                        target_name: target_name.to_string(),
                    };
                    let mut meta = JsonObject::new();
                    meta.insert(
                        "replicaPublish".to_string(),
                        serde_json::to_value(step_meta)
                            .map_err(|error| BackendError::protocol(error.to_string()))?,
                    );
                    steps.push(Step {
                        kind: StepKind::ReplicaPublish,
                        src: Some(source_file.path.to_string_lossy().into_owned()),
                        dst: Some(destination.to_string_lossy().into_owned()),
                        note_key: Some("note.replica.replace".to_string()),
                        meta,
                    });
                    rows.push(review_row(
                        target_name,
                        relative,
                        "replace",
                        &target_file.hash,
                        &source_file.hash,
                    ));
                }
            }
        }
        push_target_extra_warnings(&mut warnings, target_name, &source.files, &target.files);
    }

    if steps.is_empty() {
        warnings.push(Warning {
            key: "warn.replica.noChanges".to_string(),
            subject: String::new(),
        });
    }
    let changed_file_count = rows.len() as u64;
    if automatic {
        enforce_automatic_change_limit(changed_file_count, changed_bytes, params)?;
    }
    let mut meta = JsonObject::new();
    meta.insert("editor".to_string(), replica_editor(rows));
    meta.insert("sourceRoot".to_string(), json!(source_root));
    meta.insert("backupRoot".to_string(), json!(backup_root));
    meta.insert("unattended".to_string(), Value::Bool(params.unattended));
    meta.insert(
        "intervalMinutes".to_string(),
        Value::from(params.interval_minutes),
    );
    meta.insert(
        "changedFileCount".to_string(),
        Value::from(changed_file_count),
    );
    meta.insert("changedBytes".to_string(), Value::from(changed_bytes));
    if cache.is_some() {
        meta.insert(
            "scanIndex".to_string(),
            serde_json::to_value(&scan_stats)
                .map_err(|error| BackendError::protocol(error.to_string()))?,
        );
    }
    if params.unattended {
        meta.insert(
            "automationProfile".to_string(),
            serde_json::to_value(params)
                .map_err(|error| BackendError::protocol(error.to_string()))?,
        );
    }
    progress(ProgressEvent {
        phase: "preview_ready".to_string(),
        done: 1,
        total: 1,
        subject: String::new(),
    });
    Ok(Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps,
        warnings,
        meta,
    })
}

fn push_target_extra_warnings<A, B>(
    warnings: &mut Vec<Warning>,
    target_name: &str,
    source_files: &BTreeMap<PathBuf, A>,
    target_files: &BTreeMap<PathBuf, B>,
) {
    let mut omitted_extra_warnings = 0_usize;
    for relative in target_files.keys() {
        if !source_files.contains_key(relative) && warnings.len() < MAX_WARNINGS {
            warnings.push(Warning {
                key: "warn.replica.targetExtra".to_string(),
                subject: format!("{target_name}: {}", relative.display()),
            });
        } else if !source_files.contains_key(relative) {
            omitted_extra_warnings += 1;
        }
    }
    if omitted_extra_warnings > 0 {
        warnings.push(Warning {
            key: "warn.replica.targetExtraOverflow".to_string(),
            subject: format!("{target_name}: {omitted_extra_warnings}"),
        });
    }
}

fn replica_editor(rows: Vec<Value>) -> Value {
    json!({
        "kind":"recordTable", "layout":"compact", "titleKey":"replica.editor.title",
        "rowCountKey":"replica.editor.rowCount", "requiredFields":[], "groupField":"target",
        "selectionGroupFields":[], "columns":[
            {"key":"target","labelKey":"replica.column.target","kind":"readonly"},
            {"key":"relativePath","labelKey":"replica.column.path","kind":"readonly"},
            {"key":"action","labelKey":"replica.column.action","kind":"readonly","display":"localizedKey"},
            {"key":"beforeHash","labelKey":"replica.column.beforeHash","kind":"readonly","detail":true},
            {"key":"afterHash","labelKey":"replica.column.afterHash","kind":"readonly","detail":true}
        ], "rows":rows, "duplicateEnabled":false, "selectionEnabled":false,
        "bulkEnabled":false, "reorderable":false, "detailsEnabled":true,
        "detailsDefaultVisible":false, "copyEnabled":false
    })
}

pub(crate) fn decode_step_meta(step: &Step) -> Result<ReplicaStepMeta, BackendError> {
    if step.kind != StepKind::ReplicaPublish {
        return Err(BackendError::validation(
            "error.replica.approvedStepInvalid",
        ));
    }
    serde_json::from_value(
        step.meta
            .get("replicaPublish")
            .cloned()
            .ok_or_else(|| BackendError::validation("error.replica.approvedStepInvalid"))?,
    )
    .map_err(|_| BackendError::validation("error.replica.approvedStepInvalid"))
}

fn review_row(
    target: &str,
    relative: &Path,
    action: &str,
    before_hash: &str,
    after_hash: &str,
) -> Value {
    json!({
        "_rowId": Uuid::new_v4().to_string(),
        "target": target,
        "relativePath": relative.to_string_lossy(),
        "action": format!("replica.action.{action}"),
        "beforeHash": before_hash,
        "afterHash": after_hash,
    })
}

fn canonical_roots(params: &ReplicaPublishParams) -> Result<[PathBuf; 3], BackendError> {
    Ok([
        canonical_directory(&params.root_a, "error.replica.rootInvalid")?,
        canonical_directory(&params.root_b, "error.replica.rootInvalid")?,
        canonical_directory(&params.root_c, "error.replica.rootInvalid")?,
    ])
}

fn canonical_directory(path: &Path, key: &str) -> Result<PathBuf, BackendError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| BackendError::path_invalid(key))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BackendError::path_invalid(key));
    }
    fs::canonicalize(path).map_err(|_| BackendError::path_invalid(key))
}

fn validate_root_topology(roots: &[PathBuf; 3], backup: &Path) -> Result<(), BackendError> {
    for (index, left) in roots.iter().enumerate() {
        for right in roots.iter().skip(index + 1) {
            if left == right || left.starts_with(right) || right.starts_with(left) {
                return Err(BackendError::validation("error.replica.rootsOverlap"));
            }
        }
        if backup == left || backup.starts_with(left) || left.starts_with(backup) {
            return Err(BackendError::validation("error.replica.backupOverlap"));
        }
    }
    Ok(())
}

fn parse_root_files(value: &str) -> Result<BTreeSet<String>, BackendError> {
    let mut result = BTreeSet::new();
    for line in value
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let path = Path::new(line);
        if path.components().count() != 1 || matches!(line, "." | "..") {
            return Err(BackendError::validation("error.replica.rootFileInvalid"));
        }
        result.insert(line.to_string());
    }
    if result.len() > MAX_ROOT_FILES {
        return Err(BackendError::validation("error.replica.tooManyRootFiles"));
    }
    Ok(result)
}

mod snapshot;
use snapshot::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn params(root: &Path) -> ReplicaPublishParams {
        for name in ["a", "b", "c", "backup"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        for system in ["a", "b", "c"] {
            for discipline in DISCIPLINES {
                fs::create_dir_all(root.join(system).join(discipline)).unwrap();
            }
        }
        ReplicaPublishParams {
            root_a: root.join("a"),
            root_b: root.join("b"),
            root_c: root.join("c"),
            source_root: "a".to_string(),
            backup_root: root.join("backup"),
            root_files: "RFI Log.xlsm".to_string(),
            unattended: true,
            interval_minutes: 15,
            settle_seconds: 30,
            max_changed_files: 250,
            max_changed_mib: 2_048,
        }
    }

    #[test]
    fn preview_copies_missing_replaces_changed_and_never_deletes_extra() {
        let fixture = tempfile::tempdir().unwrap();
        let params = params(fixture.path());
        fs::write(params.root_a.join("CIV/source.pdf"), b"source").unwrap();
        fs::write(params.root_b.join("CIV/source.pdf"), b"old").unwrap();
        fs::write(params.root_c.join("CIV/extra.pdf"), b"extra").unwrap();
        fs::write(params.root_a.join("RFI Log.xlsm"), b"log").unwrap();

        let plan = preview(&params, |_| {}).unwrap();

        assert_eq!(
            plan.steps
                .iter()
                .filter(|step| step.kind == StepKind::ReplicaPublish)
                .count(),
            1
        );
        assert_eq!(
            plan.steps
                .iter()
                .filter(|step| step.kind == StepKind::Copy)
                .count(),
            3
        );
        assert!(plan.warnings.iter().any(|warning| {
            warning.key == "warn.replica.targetExtra" && warning.subject.contains("extra.pdf")
        }));
        assert!(plan.meta.contains_key("automationProfile"));
    }

    #[test]
    fn preview_rejects_overlapping_roots() {
        let fixture = tempfile::tempdir().unwrap();
        let mut params = params(fixture.path());
        params.backup_root = params.root_a.join("backup");
        fs::create_dir_all(&params.backup_root).unwrap();
        assert_eq!(
            preview(&params, |_| {}).unwrap_err().message,
            "error.replica.backupOverlap"
        );
    }

    #[test]
    fn preview_rejects_a_missing_selected_source_root_file() {
        let fixture = tempfile::tempdir().unwrap();
        let params = params(fixture.path());

        assert_eq!(
            preview(&params, |_| {}).unwrap_err().message,
            "error.replica.sourceRootFileMissing"
        );
    }

    #[test]
    fn indexed_preview_reuses_unchanged_hashes() {
        let fixture = tempfile::tempdir().unwrap();
        let params = params(fixture.path());
        fs::write(params.root_a.join("MEC/source.pdf"), b"source").unwrap();
        fs::write(params.root_a.join("RFI Log.xlsm"), b"log").unwrap();
        let index = ScanIndex::new(fixture.path().join("scan-index.sqlite3")).unwrap();

        let first = preview_with_index(&params, "owner", &index, |_| {}).unwrap();
        let second = preview_with_index(&params, "owner", &index, |_| {}).unwrap();

        let first_stats: ScanIndexStats =
            serde_json::from_value(first.meta["scanIndex"].clone()).unwrap();
        let second_stats: ScanIndexStats =
            serde_json::from_value(second.meta["scanIndex"].clone()).unwrap();
        assert_eq!(first_stats.discovered, 2);
        assert_eq!(first_stats.analyzed, 2);
        assert_eq!(first_stats.reused, 0);
        assert_eq!(second_stats.discovered, 2);
        assert_eq!(second_stats.analyzed, 0);
        assert_eq!(second_stats.reused, 2);
        assert_eq!(first.steps, second.steps);
    }

    #[test]
    fn automatic_preview_defers_a_source_that_is_still_settling() {
        let fixture = tempfile::tempdir().unwrap();
        let params = params(fixture.path());
        fs::write(params.root_a.join("RFI Log.xlsm"), b"log").unwrap();
        let index = ScanIndex::new(fixture.path().join("scan-index.sqlite3")).unwrap();

        assert_eq!(
            preview_automatic_with_index(&params, "owner", &index, |_| {})
                .unwrap_err()
                .message,
            "error.replica.sourceSettling"
        );
    }

    #[test]
    fn automatic_preview_stops_before_a_mass_change() {
        let fixture = tempfile::tempdir().unwrap();
        let mut params = params(fixture.path());
        params.max_changed_files = 1;

        assert_eq!(
            enforce_automatic_change_limit(2, 1, &params)
                .unwrap_err()
                .message,
            "error.replica.changeLimitExceeded"
        );
    }
}
