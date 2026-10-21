use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::rfi_code::normalize_candidate_number;
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 50_000;
pub const MAX_DEPTH: usize = 32;
pub const MAX_REQUESTS: usize = 500;
pub const MAX_INPUT_BYTES: usize = 64;
pub const MAX_CANDIDATES_PER_REQUEST: usize = 250;
pub const MAX_RESULTS: usize = 5_000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentType {
    FirstPage,
    Sketch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    pub requested: String,
    pub rfi_code: String,
    pub document_type: DocumentType,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resolution {
    pub requested: String,
    pub rfi_codes: Vec<String>,
    pub candidates: Vec<Candidate>,
    pub missing_first_page: bool,
    pub missing_sketch: bool,
    pub ambiguous: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Discovery {
    pub resolutions: Vec<Resolution>,
    pub candidates: Vec<Candidate>,
    pub visited: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IncludeTypes {
    pub first_page: bool,
    pub sketch: bool,
}

pub fn discover(
    root: &Path,
    inputs: &[String],
    include: IncludeTypes,
) -> Result<Discovery, &'static str> {
    if !include.first_page && !include.sketch {
        return Err("at least one document type is required");
    }
    if inputs.is_empty() || inputs.len() > MAX_REQUESTS {
        return Err("RFI request count is outside the supported range");
    }
    let root = fs::canonicalize(root).map_err(|_| "RFI System root is unavailable")?;
    if !fs::symlink_metadata(&root)
        .map_err(|_| "RFI System root cannot be inspected")?
        .is_dir()
    {
        return Err("RFI System root must be a folder");
    }
    let requests = inputs
        .iter()
        .map(|input| parse_request(input))
        .collect::<Result<Vec<_>, _>>()?;

    let mut files = Vec::new();
    let mut pending = vec![(root.clone(), 0usize)];
    let mut visited = 0usize;
    let mut truncated = false;
    while let Some((directory, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            truncated = true;
            continue;
        }
        let entries = if let Ok(entries) = fs::read_dir(&directory) {
            entries
        } else {
            truncated = true;
            continue;
        };
        for entry in entries {
            if visited >= MAX_ENTRIES {
                truncated = true;
                break;
            }
            visited += 1;
            let Ok(entry) = entry else {
                truncated = true;
                continue;
            };
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                truncated = true;
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                pending.push((path, depth + 1));
            } else if meta.is_file() && path.strip_prefix(&root).is_ok() {
                files.push(path);
            }
        }
        if truncated {
            break;
        }
    }
    files.sort();
    let scan_incomplete = truncated;
    let mut files_by_code = BTreeMap::<String, Vec<(&PathBuf, DocumentType)>>::new();
    for path in &files {
        if let Some(code) = code_for_filename(path)
            && let Some(document_type) = classify(path, &code)
        {
            files_by_code
                .entry(code)
                .or_default()
                .push((path, document_type));
        }
    }
    let codes: BTreeSet<String> = files_by_code.keys().cloned().collect();
    let mut resolutions = Vec::with_capacity(requests.len());
    let mut all_candidates = Vec::new();
    for (requested, discipline, number) in requests {
        let matching_codes: Vec<_> = codes
            .iter()
            .filter(|code| {
                code.rsplit('-').next() == Some(number.as_str())
                    && discipline
                        .as_ref()
                        .is_none_or(|wanted| discipline_for_code(code) == Some(wanted.as_str()))
            })
            .cloned()
            .collect();
        let ambiguous = discipline.is_none()
            && matching_codes
                .iter()
                .map(|code| discipline_for_code(code).unwrap_or(""))
                .collect::<BTreeSet<_>>()
                .len()
                > 1;
        let mut candidates = Vec::new();
        let mut found_first_page = false;
        let mut found_sketch = false;
        for code in &matching_codes {
            let Some(matched_files) = files_by_code.get(code) else {
                continue;
            };
            for (file, document_type) in matched_files {
                let document_type = *document_type;
                if (document_type == DocumentType::FirstPage && !include.first_page)
                    || (document_type == DocumentType::Sketch && !include.sketch)
                {
                    continue;
                }
                match document_type {
                    DocumentType::FirstPage => found_first_page = true,
                    DocumentType::Sketch => found_sketch = true,
                }
                if candidates.len() == MAX_CANDIDATES_PER_REQUEST {
                    truncated = true;
                    continue;
                }
                candidates.push(Candidate {
                    requested: requested.clone(),
                    rfi_code: code.clone(),
                    document_type,
                    path: (*file).clone(),
                });
            }
        }
        candidates.sort_by(|a, b| a.path.cmp(&b.path));
        let missing_first_page = !scan_incomplete && include.first_page && !found_first_page;
        let missing_sketch = !scan_incomplete && include.sketch && !found_sketch;
        all_candidates.extend(candidates.iter().cloned());
        resolutions.push(Resolution {
            requested,
            rfi_codes: matching_codes,
            candidates,
            missing_first_page,
            missing_sketch,
            ambiguous,
        });
    }
    all_candidates.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.document_type.cmp(&b.document_type))
            .then(a.requested.cmp(&b.requested))
    });

    all_candidates.dedup_by(|a, b| {
        a.path == b.path && a.document_type == b.document_type && a.requested == b.requested
    });
    if all_candidates.len() > MAX_RESULTS {
        all_candidates.truncate(MAX_RESULTS);
        truncated = true;
    }
    Ok(Discovery {
        resolutions,
        candidates: all_candidates,
        visited,
        truncated,
    })
}

pub fn initially_selected(resolution: &Resolution, candidate: &Candidate) -> bool {
    !resolution.ambiguous
        && resolution
            .candidates
            .iter()
            .filter(|row| row.document_type == candidate.document_type)
            .count()
            == 1
}

fn is_supported_discipline(discipline: &str) -> bool {
    matches!(discipline, "CIV" | "MEC" | "ELE" | "PRC")
}
fn parse_request(input: &str) -> Result<(String, Option<String>, String), &'static str> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_INPUT_BYTES {
        return Err("RFI request value is invalid");
    }
    let value = trimmed.to_ascii_uppercase();
    let (discipline, number) = if let Some(suffix) = value.strip_prefix("AZE-SUN-INT-QMS-RFI-") {
        let Some((discipline, number)) = suffix.split_once('-') else {
            return Err("RFI number must be a four-digit number or canonical full code");
        };
        let Some(number) = normalize_candidate_number(number) else {
            return Err("RFI number must be a four-digit number or canonical full code");
        };
        if !is_supported_discipline(discipline) {
            return Err("RFI number must be a four-digit number or canonical full code");
        }
        (Some(discipline.to_string()), number)
    } else if let Some((discipline, number)) = value.split_once('-') {
        let Some(number) = normalize_candidate_number(number) else {
            return Err("RFI number must be a four-digit number or canonical full code");
        };
        if !is_supported_discipline(discipline) {
            return Err("RFI number must be a four-digit number or canonical full code");
        }
        (Some(discipline.to_string()), number)
    } else if let Some(number) = normalize_candidate_number(&value) {
        (None, number)
    } else {
        return Err("RFI number must be a four-digit number or canonical full code");
    };
    Ok((input.trim().to_string(), discipline, number))
}

fn discipline_for_code(code: &str) -> Option<&str> {
    code.strip_prefix("AZE-SUN-INT-QMS-RFI-")?.split('-').next()
}

fn code_for_filename(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.to_ascii_uppercase();
    let marker = "AZE-SUN-INT-QMS-RFI-";
    let start = name.find(marker)?;
    let fragment = &name[start..];
    let tail = fragment
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .next()?;
    let suffix = tail.strip_prefix(marker)?;
    let (discipline, number) = suffix.split_once('-')?;
    let number = normalize_candidate_number(number)?;
    if !is_supported_discipline(discipline) {
        return None;
    }
    Some(format!("{marker}{discipline}-{number}"))
}

fn classify(path: &Path, code: &str) -> Option<DocumentType> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let code = code.to_ascii_lowercase();
    if ext == "docx" && stem == code {
        return Some(DocumentType::FirstPage);
    }
    if ext == "pdf" {
        let rest = stem.strip_prefix(&code)?;
        if rest
            .trim_matches(|character: char| matches!(character, ' ' | '_' | '-'))
            .eq("sketch")
        {
            return Some(DocumentType::Sketch);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    fn temp() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "docflow-rfi-print-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn print_finder_normalizes_short_requests_and_filename_suffixes() {
        assert_eq!(parse_request("9").unwrap().2, "0009");
        assert_eq!(parse_request("91").unwrap().2, "0091");
        assert_eq!(parse_request("901").unwrap().2, "0901");
        assert_eq!(parse_request("0901").unwrap().2, "0901");
        assert_eq!(
            parse_request("AZE-SUN-INT-QMS-RFI-CIV-901").unwrap().2,
            "0901"
        );
        assert!(parse_request("09010").is_err());
        assert!(parse_request("ITP-CIV-901").is_err());
        assert!(parse_request("2026-09-01").is_err());
        assert_eq!(
            code_for_filename(Path::new("AZE-SUN-INT-QMS-RFI-CIV-901.docx")).as_deref(),
            Some("AZE-SUN-INT-QMS-RFI-CIV-0901")
        );
        assert!(code_for_filename(Path::new("AZE-SUN-INT-QMS-RFI-CIV-09010.docx")).is_none());
    }

    #[test]
    fn exact_matching_ambiguity_versions_and_missing_types() {
        let root = temp();
        let civ = "AZE-SUN-INT-QMS-RFI-CIV-1319";
        let mec = "AZE-SUN-INT-QMS-RFI-MEC-1319";
        for relative in [
            format!("CIV/{civ}.docx"),
            format!("CIV/old/{civ}.DOCX"),
            format!("CIV/{civ}_sKeTcH.PDF"),
            format!("MEC/{mec}.docx"),
            "CIV/AZE-SUN-INT-QMS-RFI-CIV-13190.docx".to_string(),
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"").unwrap();
        }
        let include = IncludeTypes {
            first_page: true,
            sketch: true,
        };
        let result = discover(&root, &["1319".into()], include).unwrap();
        assert!(result.resolutions[0].ambiguous);
        assert_eq!(result.resolutions[0].candidates.len(), 4);
        assert!(
            result.resolutions[0]
                .candidates
                .iter()
                .all(|candidate| { !initially_selected(&result.resolutions[0], candidate) })
        );
        assert!(!result.resolutions[0].missing_first_page);
        let selected = discover(&root, &[civ.into()], include).unwrap();
        assert_eq!(selected.resolutions[0].candidates.len(), 3);
        assert!(!selected.resolutions[0].ambiguous);
        assert!(selected.resolutions[0].candidates.iter().any(|candidate| {
            candidate.document_type == DocumentType::Sketch
                && initially_selected(&selected.resolutions[0], candidate)
        }));
        assert!(
            selected.resolutions[0]
                .candidates
                .iter()
                .filter(|candidate| { candidate.document_type == DocumentType::FirstPage })
                .all(|candidate| !initially_selected(&selected.resolutions[0], candidate))
        );
        let short_code = discover(&root, &["CIV-1319".into()], include).unwrap();
        assert_eq!(short_code.resolutions[0].candidates.len(), 3);
        let missing = discover(
            &root,
            &["1318".into()],
            IncludeTypes {
                first_page: true,
                sketch: false,
            },
        )
        .unwrap();
        assert!(missing.resolutions[0].missing_first_page);
        let missing_sketch = discover(
            &root,
            &["1318".into()],
            IncludeTypes {
                first_page: false,
                sketch: true,
            },
        )
        .unwrap();
        assert!(missing_sketch.resolutions[0].missing_sketch);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_unsupported_disciplines_in_inputs_and_filenames() {
        assert!(parse_request("XYZ-1319").is_err());
        assert!(parse_request("CLV-1319").is_err());
        assert!(parse_request("AZE-SUN-INT-QMS-RFI-XYZ-1319").is_err());
        let root = temp();
        let unsupported = root.join("AZE-SUN-INT-QMS-RFI-XYZ-1319.docx");
        fs::write(&unsupported, b"").unwrap();
        let result = discover(
            &root,
            &["1319".into()],
            IncludeTypes {
                first_page: true,
                sketch: false,
            },
        )
        .unwrap();
        assert!(result.candidates.is_empty());
        assert!(result.resolutions[0].missing_first_page);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn excludes_symlinked_files_outside_selected_root() {
        use std::os::unix::fs::symlink;
        let root = temp();
        let outside = temp();
        let escaped = outside.join("AZE-SUN-INT-QMS-RFI-CIV-1319.docx");
        fs::write(&escaped, b"").unwrap();
        symlink(&escaped, root.join("AZE-SUN-INT-QMS-RFI-CIV-1319.docx")).unwrap();
        let result = discover(
            &root,
            &["CIV-1319".into()],
            IncludeTypes {
                first_page: true,
                sketch: false,
            },
        )
        .unwrap();
        assert!(result.candidates.is_empty());
        assert!(result.resolutions[0].missing_first_page);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
