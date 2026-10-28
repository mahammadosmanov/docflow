use super::*;

pub(super) fn empty_plan() -> Plan {
    Plan {
        plan_id: String::new(),
        op_id: OPERATION_ID.to_string(),
        steps: Vec::new(),
        warnings: Vec::new(),
        meta: JsonObject::from_iter([
            ("sourceReadOnly".to_string(), Value::Bool(true)),
            ("preservesDirectoryLayout".to_string(), Value::Bool(true)),
            (
                "confidenceThreshold".to_string(),
                Value::from(MIN_ORIENTATION_CONFIDENCE),
            ),
        ]),
    }
}

pub(super) fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BackendError> {
    let path = fs::canonicalize(path)
        .map_err(|_| BackendError::path_invalid(format!("{label} does not exist")))?;
    if !path.is_dir() {
        return Err(BackendError::path_invalid(format!(
            "{label} is not a directory"
        )));
    }
    Ok(path)
}

pub(super) fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

pub(super) fn normalize_rotation(rotation: i64) -> Result<i64, BackendError> {
    let normalized = rotation.rem_euclid(360);
    if matches!(normalized, 0 | 90 | 180 | 270) {
        Ok(normalized)
    } else {
        Err(BackendError::validation(
            "PDF page rotation is not a right angle",
        ))
    }
}

pub(super) fn warning(key: &str, subject: &str) -> Warning {
    Warning {
        key: key.to_string(),
        subject: subject.to_string(),
    }
}

pub(super) fn truncate(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push('…');
    }
    result
}
