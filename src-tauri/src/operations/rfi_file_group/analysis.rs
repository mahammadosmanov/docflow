//! Filename and content analysis that detects RFI codes in source files.

use super::*;

#[derive(Clone, Debug)]
pub(super) struct Detection {
    pub(super) code: String,
    pub(super) method_key: String,
    pub(super) confidence: f64,
}

#[derive(Clone, Debug)]
pub(super) struct FileAnalysis {
    pub(super) path: PathBuf,
    pub(super) detections: Vec<Detection>,
    pub(super) issue: Option<String>,
    pub(super) is_mail: bool,
}

pub(super) fn discover_files(root: &Path) -> Result<Vec<PathBuf>, BackendError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)
        .map_err(|_| BackendError::path_invalid("RFI file source could not be read"))?
    {
        let entry = entry
            .map_err(|_| BackendError::path_invalid("RFI file source entry could not be read"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| BackendError::path_invalid("RFI file source entry changed"))?;
        if metadata.file_type().is_symlink() {
            return Err(BackendError::path_invalid(
                "RFI file source contains a symbolic link",
            ));
        }
        let hidden = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'));
        if metadata.is_file() && !hidden {
            files.push(entry.path());
            if files.len() > MAX_FILES {
                return Err(BackendError::validation(format!(
                    "RFI file source exceeds the {MAX_FILES}-file limit"
                )));
            }
        }
    }
    Ok(files)
}

pub(super) fn analyze_file<A: OcrAdapter>(path: &Path, ocr: &A) -> FileAnalysis {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let filename_codes = exact_codes(file_name);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_mail = matches!(extension.as_str(), "eml" | "msg");
    let inspect_content = filename_codes.is_empty() || is_mail;
    let mut detections = filename_codes
        .into_iter()
        .map(|code| Detection {
            code,
            method_key: "rfi.fileGroup.method.filename".to_string(),
            confidence: 1.0,
        })
        .map(|detection| (detection.code.clone(), detection))
        .collect::<BTreeMap<_, _>>();
    let mut issue = None;
    if inspect_content {
        let content = match extension.as_str() {
            "pdf" => pdf_codes(path, ocr),
            "docx" | "xlsx" | "xlsm" | "pptx" => package_codes(path),
            _ => raw_codes(path),
        };
        match content {
            Ok(codes) => {
                let method_key = match extension.as_str() {
                    "pdf" => "rfi.fileGroup.method.pdf",
                    "eml" | "msg" => "rfi.fileGroup.method.mail",
                    "docx" | "xlsx" | "xlsm" | "pptx" => "rfi.fileGroup.method.office",
                    _ => "rfi.fileGroup.method.content",
                };
                for code in codes {
                    detections.entry(code.clone()).or_insert(Detection {
                        code,
                        method_key: method_key.to_string(),
                        confidence: 1.0,
                    });
                }
            }
            Err(error) => {
                if is_mail {
                    detections.clear();
                }
                issue = Some(error);
            }
        }
    }
    FileAnalysis {
        path: path.to_path_buf(),
        detections: detections.into_values().collect(),
        issue,
        is_mail,
    }
}

pub(super) fn summarize_file_names(file_names: &BTreeSet<String>) -> String {
    const SHOWN: usize = 3;
    let shown = file_names.iter().take(SHOWN).cloned().collect::<Vec<_>>();
    let remaining = file_names.len().saturating_sub(shown.len());
    if remaining == 0 {
        shown.join(", ")
    } else {
        format!("{} (+{remaining})", shown.join(", "))
    }
}

pub(super) fn pdf_codes<A: OcrAdapter>(path: &Path, ocr: &A) -> Result<BTreeSet<String>, String> {
    let facts = pdf::inspect(path).map_err(|error| error.to_string())?;
    if facts.page_count > MAX_PDF_PAGES {
        return Err(format!(
            "PDF exceeds the {MAX_PDF_PAGES}-page inspection limit"
        ));
    }
    let mut codes = BTreeSet::new();
    for page_index in 0..facts.page_count {
        match ocr.read_page(path, page_index) {
            Ok(read) if read.matched.method == RfiMethod::Strict => {
                codes.insert(read.matched.code);
            }
            Ok(_) => {}
            Err(OcrFailure::EngineMissing) => return Err(ocr.diagnostics()),
            Err(OcrFailure::Document(error)) => return Err(error),
        }
    }
    Ok(codes)
}

pub(super) fn package_codes(path: &Path) -> Result<BTreeSet<String>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("metadata: {error}"))?;
    if metadata.len() > MAX_PACKAGE_BYTES {
        return Err("Office package exceeds the size limit".to_string());
    }
    let file = fs::File::open(path).map_err(|error| format!("open: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("Office package: {error}"))?;
    if archive.len() > MAX_PACKAGE_ENTRIES {
        return Err("Office package has too many entries".to_string());
    }
    let mut total = 0_u64;
    let mut codes = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Office package entry: {error}"))?;
        let name = entry
            .name()
            .map_err(|error| format!("Office package entry name: {error}"))?;
        if !name.to_ascii_lowercase().ends_with(".xml") {
            continue;
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| "Office package text size overflow".to_string())?;
        if total > MAX_PACKAGE_TEXT_BYTES {
            return Err("Office package text exceeds the size limit".to_string());
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Office package read: {error}"))?;
        codes.extend(exact_codes(&xml_text(&bytes)?));
    }
    Ok(codes)
}

pub(super) fn xml_text(bytes: &[u8]) -> Result<String, String> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(value)) => {
                let value = value
                    .decode()
                    .map_err(|error| format!("Office package XML text: {error}"))?;
                text.push_str(&value);
            }
            Ok(Event::CData(value)) => {
                let value = value
                    .decode()
                    .map_err(|error| format!("Office package XML CDATA: {error}"))?;
                text.push_str(&value);
            }
            Ok(Event::Empty(value)) if xml_local_name(value.name().as_ref()) == b"br" => {
                text.push('\n');
            }
            Ok(Event::End(value))
                if matches!(xml_local_name(value.name().as_ref()), b"p" | b"tc" | b"tr") =>
            {
                text.push('\n');
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("Office package XML: {error}")),
            _ => {}
        }
    }
    Ok(text)
}

pub(super) fn xml_local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

pub(super) fn raw_codes(path: &Path) -> Result<BTreeSet<String>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("metadata: {error}"))?;
    if metadata.len() > MAX_RAW_BYTES {
        return Err("file exceeds the content-inspection size limit".to_string());
    }
    let bytes = fs::read(path).map_err(|error| format!("read: {error}"))?;
    let mut codes = exact_codes(&String::from_utf8_lossy(&bytes));
    for offset in 0..=1 {
        if bytes.len() <= offset + 1 {
            continue;
        }
        let little = bytes[offset..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<_>>();
        let big = bytes[offset..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect::<Vec<_>>();
        codes.extend(exact_codes(&String::from_utf16_lossy(&little)));
        codes.extend(exact_codes(&String::from_utf16_lossy(&big)));
    }
    Ok(codes)
}

pub(super) fn exact_codes(value: &str) -> BTreeSet<String> {
    static RFI: OnceLock<Regex> = OnceLock::new();
    let text = value.nfc().collect::<String>().to_uppercase();
    let regex = RFI.get_or_init(|| {
        Regex::new(r"AZE-SUN-INT-QMS-RFI-([A-Z]{3})-([0-9]{1,4})")
            .expect("exact RFI regex is valid")
    });
    let mut codes = shared_exact_codes(&text);
    for captures in regex.captures_iter(&text) {
        let matched = captures.get(0).expect("RFI match exists");
        let before_invalid = text[..matched.start()]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
        let after_invalid = text[matched.end()..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric);
        let Some(number) = normalize_candidate_number(&captures[2]) else {
            continue;
        };
        let canonical = format!("AZE-SUN-INT-QMS-RFI-{}-{number}", &captures[1]);
        if before_invalid || after_invalid || !codes.contains(&canonical) {
            continue;
        }
        let discipline = &captures[1];
        codes.insert(canonical);
        let bytes = text.as_bytes();
        let mut cursor = matched.end();
        loop {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if !bytes
                .get(cursor)
                .is_some_and(|byte| matches!(*byte, b'_' | b'/' | b',' | b';'))
            {
                break;
            }
            cursor += 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            let start = cursor;
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
            let Some(raw_number) = text.get(start..cursor) else {
                break;
            };
            let Some(number) = normalize_candidate_number(raw_number) else {
                break;
            };
            if text[cursor..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
            {
                break;
            }
            codes.insert(format!("AZE-SUN-INT-QMS-RFI-{discipline}-{number}"));
        }
    }
    codes
}
