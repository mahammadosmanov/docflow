//! Capability root opening, journal root identity and step path validation.

use super::*;

pub(super) fn prepare_roots(
    journals: &JournalStore,
    record: &mut JournalRecord,
) -> Result<Vec<CapabilityRoot>, BackendError> {
    if record.grant_roots.is_empty() && !record.grant_root_identities.is_empty() {
        return Err(BackendError::journal_invalid(
            "journal root identity metadata has no grant roots",
        ));
    }
    if record.grant_roots.is_empty() {
        record.grant_roots = derive_legacy_roots(record)?
            .into_iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect();
    }
    let roots = open_roots(&record.grant_roots)?;
    if record.grant_root_identities.is_empty() {
        record.grant_root_identities = root_identity_records(&roots)?;
        journals.write(record)?;
        return Ok(roots);
    }
    validate_root_identities(&roots, &record.grant_root_identities)?;
    Ok(roots)
}

pub(super) fn open_roots(raw_roots: &[String]) -> Result<Vec<CapabilityRoot>, BackendError> {
    if raw_roots.is_empty() {
        return Err(BackendError::journal_invalid(
            "journal has no capability roots for safe undo",
        ));
    }
    let mut roots = Vec::new();
    for raw in raw_roots {
        let root = PathBuf::from(raw);
        reject_lexical_path(&root)?;
        let canonical = fs::canonicalize(&root)
            .map_err(|_| BackendError::journal_invalid("journal grant root is unavailable"))?;
        if canonical != root {
            return Err(BackendError::journal_invalid(
                "journal grant root path is not canonical",
            ));
        }
        if roots
            .iter()
            .any(|known: &CapabilityRoot| known.canonical() == root)
        {
            return Err(BackendError::journal_invalid(
                "journal contains duplicate grant roots",
            ));
        }
        roots.push(
            CapabilityRoot::open(&root)
                .map_err(|_| BackendError::journal_invalid("journal grant root is unavailable"))?,
        );
    }
    Ok(roots)
}

pub(super) fn root_identity_records(
    roots: &[CapabilityRoot],
) -> Result<Vec<GrantRootIdentity>, BackendError> {
    roots
        .iter()
        .map(|root| {
            Ok(GrantRootIdentity {
                root: root.canonical().to_string_lossy().into_owned(),
                identity: root.identity().map_err(|_| {
                    BackendError::journal_invalid("journal grant root identity is unavailable")
                })?,
            })
        })
        .collect()
}

pub(super) fn validate_root_identities(
    roots: &[CapabilityRoot],
    identities: &[GrantRootIdentity],
) -> Result<(), BackendError> {
    if roots.len() != identities.len() {
        return Err(BackendError::journal_invalid(
            "journal grant root identity metadata is incomplete",
        ));
    }
    for root in roots {
        let path = root.canonical().to_string_lossy();
        let mut matching = identities.iter().filter(|entry| entry.root == path);
        let expected = matching.next().ok_or_else(|| {
            BackendError::journal_invalid("journal grant root identity metadata is incomplete")
        })?;
        if matching.next().is_some() {
            return Err(BackendError::journal_invalid(
                "journal contains duplicate grant root identities",
            ));
        }
        let actual = root.identity().map_err(|_| {
            BackendError::journal_invalid("journal grant root identity is unavailable")
        })?;
        if actual != expected.identity {
            return Err(BackendError::journal_invalid(
                "journal grant root object identity changed",
            ));
        }
    }
    Ok(())
}

pub(super) fn derive_legacy_roots(record: &JournalRecord) -> Result<Vec<PathBuf>, BackendError> {
    if record.steps.is_empty() {
        return Err(BackendError::journal_invalid(
            "legacy journal has no reversible steps",
        ));
    }
    let mut roots = Vec::new();
    for step in &record.steps {
        if !matches!(
            step.kind,
            StepKind::Mkdir
                | StepKind::Move
                | StepKind::Rename
                | StepKind::Copy
                | StepKind::Write
                | StepKind::RestoreBackup
        ) {
            return Err(BackendError::journal_invalid(
                "legacy journal contains an unmigrated executable action",
            ));
        }
        for raw in [step.src.as_deref(), step.dst.as_deref()]
            .into_iter()
            .flatten()
        {
            let path = PathBuf::from(raw);
            reject_lexical_path(&path)?;
            let parent = path.parent().ok_or_else(|| {
                BackendError::journal_invalid("legacy journal path has no safe parent root")
            })?;
            if parent.parent().is_none() {
                return Err(BackendError::journal_invalid(
                    "legacy journal would require a filesystem root grant",
                ));
            }
            let parent = parent.to_path_buf();
            if !roots.contains(&parent) {
                roots.push(parent);
            }
        }
    }
    if roots.is_empty() {
        return Err(BackendError::journal_invalid(
            "legacy journal has no safe path roots",
        ));
    }
    roots.sort_by_key(|root| root.components().count());
    let mut non_redundant = Vec::<PathBuf>::new();
    for root in roots {
        if !non_redundant.iter().any(|known| root.starts_with(known)) {
            non_redundant.push(root);
        }
    }
    Ok(non_redundant)
}

pub(super) fn validate_step(step: &Step, roots: &[CapabilityRoot]) -> Result<(), BackendError> {
    match step.kind {
        StepKind::Mkdir => {
            validate_path(&required_journal_path(step.dst.as_deref())?, roots, true)?;
        }
        StepKind::Move
        | StepKind::Rename
        | StepKind::Copy
        | StepKind::Write
        | StepKind::RestoreBackup => {
            if let Some(source) = step.src.as_deref() {
                validate_path(&PathBuf::from(source), roots, true)?;
            }
            validate_path(&required_journal_path(step.dst.as_deref())?, roots, true)?;
        }
        _ => {
            return Err(BackendError::journal_invalid(
                "journal contains an unmigrated executable action",
            ));
        }
    }
    Ok(())
}

pub(super) fn required_journal_path(raw: Option<&str>) -> Result<PathBuf, BackendError> {
    raw.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| BackendError::journal_invalid("journal step path is missing"))
}

pub(super) fn validate_path(
    path: &Path,
    roots: &[CapabilityRoot],
    _allow_missing: bool,
) -> Result<(), BackendError> {
    reject_lexical_path(path)?;
    for root in roots {
        if path == root.canonical() || root.authorize(path).is_ok() {
            return Ok(());
        }
    }
    Err(BackendError::journal_invalid(
        "journal step escapes its original granted roots",
    ))
}

pub(super) fn anchor_path_io(path: &Path, roots: &[CapabilityRoot]) -> io::Result<AnchoredPath> {
    for root in roots {
        if let Ok(path) = root.authorize(path)
            && let Ok(anchor) = path.anchor()
        {
            return Ok(anchor);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "journal path is unavailable through its pinned capability root",
    ))
}

pub(super) fn reject_lexical_path(path: &Path) -> Result<(), BackendError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(BackendError::journal_invalid(
            "journal contains an invalid path",
        ));
    }
    Ok(())
}
