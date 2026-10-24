//! Read-only classification of a mail's attachment names for the publish flow.
//! Evidence is the exact RFI code in each PDF/DOCX name; subject and body are never used.

use std::collections::BTreeMap;

use crate::rfi_code::exact_codes;

#[derive(Debug, Default, Eq, PartialEq)]
pub struct RfiAttachments {
    pub pdf: Vec<String>,
    pub docx: Vec<String>,
}

impl RfiAttachments {
    /// Exactly one PDF and one DOCX; anything else needs review.
    pub fn is_complete(&self) -> bool {
        self.pdf.len() == 1 && self.docx.len() == 1
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct IntakeAnalysis {
    pub rfis: BTreeMap<String, RfiAttachments>,
    /// PDF/DOCX names carrying two or more RFI codes: never guessed.
    pub ambiguous: Vec<String>,
    /// Names without a PDF/DOCX extension or without any exact RFI code.
    pub ignored: Vec<String>,
}

impl IntakeAnalysis {
    pub fn is_publish_candidate(&self) -> bool {
        !self.rfis.is_empty()
    }
}

pub fn analyze(names: &[&str]) -> IntakeAnalysis {
    let mut out = IntakeAnalysis::default();
    for &name in names {
        let Some((stem, ext)) = name.rsplit_once('.') else {
            out.ignored.push(name.to_string());
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if (ext != "pdf" && ext != "docx") || name.contains(['/', '\\']) {
            out.ignored.push(name.to_string());
            continue;
        }
        let codes = exact_codes(stem);
        match codes.len() {
            0 => out.ignored.push(name.to_string()),
            1 => {
                let code = codes.into_iter().next().expect("one code");
                let entry = out.rfis.entry(code).or_default();
                if ext == "pdf" {
                    entry.pdf.push(name.to_string());
                } else {
                    entry.docx.push(name.to_string());
                }
            }
            _ => out.ambiguous.push(name.to_string()),
        }
    }
    out
}

/// One RFI Log row reduced to what intake needs: its canonical code and current status text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogRow {
    pub code: String,
    pub status: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    /// Complete PDF+DOCX pair and every matching Log row has a blank status.
    Ready,
    /// PDF or DOCX is missing, or there is more than one of either.
    IncompleteFiles,
    /// No exact RFI Log row for this code.
    NotInLog,
    /// A matching Log row already has a status (Ongoing, Completed, or other).
    StatusNotBlank,
}

pub fn match_log(analysis: &IntakeAnalysis, rows: &[LogRow]) -> BTreeMap<String, Disposition> {
    analysis
        .rfis
        .iter()
        .map(|(code, files)| {
            let matching: Vec<&LogRow> = rows.iter().filter(|row| row.code == *code).collect();
            let disposition = if !files.is_complete() {
                Disposition::IncompleteFiles
            } else if matching.is_empty() {
                Disposition::NotInLog
            } else if matching.iter().any(|row| !row.status.trim().is_empty()) {
                Disposition::StatusNotBlank
            } else {
                Disposition::Ready
            };
            (code.clone(), disposition)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEC: &str = "AZE-SUN-INT-QMS-RFI-MEC-0876";

    #[test]
    fn pairs_pdf_and_docx_by_exact_code_regardless_of_order() {
        let a = analyze(&[&format!("{MEC}.pdf"), &format!("{MEC}.docx")]);
        let rfi = &a.rfis[MEC];
        assert!(rfi.is_complete());
        assert!(a.is_publish_candidate());
        assert!(a.ambiguous.is_empty() && a.ignored.is_empty());
        let reversed = analyze(&[&format!("{MEC}.DOCX"), &format!("{MEC}.PDF")]);
        assert!(reversed.rfis[MEC].is_complete());
    }

    #[test]
    fn short_suffix_is_padded_and_trailing_text_is_allowed() {
        let a = analyze(&["AZE-SUN-INT-QMS-RFI-MEC-876_Sketch.pdf"]);
        assert_eq!(a.rfis[MEC].pdf, ["AZE-SUN-INT-QMS-RFI-MEC-876_Sketch.pdf"]);
        assert!(!a.rfis[MEC].is_complete());
    }

    #[test]
    fn multiple_rfis_group_separately() {
        let other = "AZE-SUN-INT-QMS-RFI-CIV-0412";
        let a = analyze(&[
            &format!("{MEC}.pdf"),
            &format!("{MEC}.docx"),
            &format!("{other}.pdf"),
        ]);
        assert_eq!(a.rfis.len(), 2);
        assert!(a.rfis[MEC].is_complete());
        assert!(!a.rfis[other].is_complete());
    }

    #[test]
    fn two_codes_in_one_name_is_ambiguous_not_guessed() {
        let name = format!("{MEC}_AZE-SUN-INT-QMS-RFI-MEC-0877.pdf");
        let a = analyze(&[&name]);
        assert!(a.rfis.is_empty());
        assert_eq!(a.ambiguous, [name]);
        assert!(!a.is_publish_candidate());
    }

    #[test]
    fn signature_images_and_unrelated_files_are_ignored() {
        let a = analyze(&[
            "image001.png",
            "Report.pdf",
            &format!("{MEC}.xlsx"),
            "AZE-SUN-INT-QMS-RFI-XYZ-0001.pdf",
            "AZE-SUN-INT-QMS-RFI-MEC-08760.pdf",
        ]);
        assert!(a.rfis.is_empty());
        assert_eq!(a.ignored.len(), 5);
    }

    #[test]
    fn path_like_names_are_ignored() {
        let a = analyze(&[&format!("..\\{MEC}.pdf"), &format!("a/{MEC}.pdf")]);
        assert!(a.rfis.is_empty());
        assert_eq!(a.ignored.len(), 2);
    }

    fn row(code: &str, status: &str) -> LogRow {
        LogRow {
            code: code.to_string(),
            status: status.to_string(),
        }
    }

    fn pair(code: &str) -> Vec<String> {
        vec![format!("{code}.pdf"), format!("{code}.docx")]
    }

    fn run(names: &[String], rows: &[LogRow]) -> BTreeMap<String, Disposition> {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        match_log(&analyze(&refs), rows)
    }

    #[test]
    fn blank_status_with_full_pair_is_ready() {
        let out = run(
            &pair(MEC),
            &[
                row(MEC, ""),
                row("AZE-SUN-INT-QMS-RFI-MEC-0001", "Completed"),
            ],
        );
        assert_eq!(out[MEC], Disposition::Ready);
    }

    #[test]
    fn whitespace_only_status_counts_as_blank() {
        assert_eq!(run(&pair(MEC), &[row(MEC, "  ")])[MEC], Disposition::Ready);
    }

    #[test]
    fn missing_log_row_is_reported_and_never_ready() {
        assert_eq!(run(&pair(MEC), &[])[MEC], Disposition::NotInLog);
    }

    #[test]
    fn any_nonblank_status_blocks_even_when_another_row_is_blank() {
        let out = run(&pair(MEC), &[row(MEC, ""), row(MEC, "Ongoing")]);
        assert_eq!(out[MEC], Disposition::StatusNotBlank);
    }

    #[test]
    fn incomplete_files_win_over_log_state() {
        let out = run(&[format!("{MEC}.pdf")], &[row(MEC, "")]);
        assert_eq!(out[MEC], Disposition::IncompleteFiles);
    }
}
