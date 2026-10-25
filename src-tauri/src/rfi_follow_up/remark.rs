//! Follow Up row remarks: a generated summary of which RFIs in a row are
//! Completed and which are not, written only when a cell holds several RFIs and at
//! least one is not Completed. Completed RFIs come first, emphasized; the rest follow
//! in plain text. Example: `CIV-0021,0022 Completed CIV-0043 In Progress`.

use std::sync::LazyLock;

use regex::Regex;

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RemarkState {
    Completed,
    InProgress,
    Pending,
}

impl RemarkState {
    fn label(self) -> &'static str {
        match self {
            Self::Completed => "Completed",
            Self::InProgress => "In Progress",
            Self::Pending => "Pending",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RemarkRun {
    pub(super) text: String,
    pub(super) emphasized: bool,
}

pub(super) fn runs_text(runs: &[RemarkRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// Groups consecutive same-discipline codes: `CIV-0021,0022`, `CIV-0021,MEC-0003`.
fn group_codes(codes: &[&str]) -> String {
    let mut out = String::new();
    let mut previous: Option<&str> = None;
    for code in codes {
        let (discipline, number) = code.rsplit_once('-').unwrap_or(("", code));
        if previous == Some(discipline) {
            out.push(',');
            out.push_str(number);
        } else {
            if previous.is_some() {
                out.push(',');
            }
            out.push_str(code);
        }
        previous = Some(discipline);
    }
    out
}

/// `entries` are short codes in row order. Returns the runs to write, or an empty
/// vector when the row needs no remark.
pub(super) fn build_remark(entries: &[(String, RemarkState)]) -> Vec<RemarkRun> {
    let mut runs = Vec::new();
    for state in [
        RemarkState::Completed,
        RemarkState::InProgress,
        RemarkState::Pending,
    ] {
        let codes: Vec<&str> = entries
            .iter()
            .filter(|(_, entry_state)| *entry_state == state)
            .map(|(code, _)| code.as_str())
            .collect();
        if codes.is_empty() {
            continue;
        }
        let separator = if runs.is_empty() { "" } else { " " };
        runs.push(RemarkRun {
            text: format!("{separator}{} {}", group_codes(&codes), state.label()),
            emphasized: state == RemarkState::Completed,
        });
    }
    runs
}

/// True for text DocFlow itself writes; only such text may be replaced or cleared.
pub(super) fn is_app_remark(text: &str) -> bool {
    static SEGMENT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"\A[A-Z]{3}-[0-9]{4}(?:,(?:[A-Z]{3}-)?[0-9]{4})* (?:Completed|In Progress|Pending)(?: [A-Z]{3}-[0-9]{4}(?:,(?:[A-Z]{3}-)?[0-9]{4})* (?:Completed|In Progress|Pending))*\z",
        )
        .unwrap()
    });
    SEGMENT.is_match(text)
}

/// A remark is needed when some cell holds at least two RFIs and not all are Completed.
pub(super) fn needs_remark(cells: &[Vec<RemarkState>]) -> bool {
    cells.iter().any(|states| {
        states.len() >= 2 && states.iter().any(|state| *state != RemarkState::Completed)
    })
}

#[cfg(test)]
mod tests {
    use super::RemarkState::{Completed, InProgress, Pending};
    use super::*;

    fn entry(code: &str, state: RemarkState) -> (String, RemarkState) {
        (code.to_string(), state)
    }

    #[test]
    fn completed_first_emphasized_then_in_progress_plain() {
        let runs = build_remark(&[
            entry("CIV-0021", Completed),
            entry("CIV-0043", InProgress),
            entry("CIV-0022", Completed),
        ]);
        assert_eq!(
            runs,
            [
                RemarkRun {
                    text: "CIV-0021,0022 Completed".into(),
                    emphasized: true
                },
                RemarkRun {
                    text: " CIV-0043 In Progress".into(),
                    emphasized: false
                },
            ]
        );
        assert_eq!(
            runs_text(&runs),
            "CIV-0021,0022 Completed CIV-0043 In Progress"
        );
    }

    #[test]
    fn without_completed_rfis_the_text_starts_without_a_separator() {
        let runs = build_remark(&[entry("CIV-0043", InProgress), entry("CIV-0044", Pending)]);
        assert_eq!(runs_text(&runs), "CIV-0043 In Progress CIV-0044 Pending");
        assert!(runs.iter().all(|run| !run.emphasized));
    }

    #[test]
    fn mixed_disciplines_keep_their_prefix() {
        let runs = build_remark(&[
            entry("CIV-0021", Completed),
            entry("MEC-0003", Completed),
            entry("MEC-0004", Completed),
            entry("CIV-0050", InProgress),
        ]);
        assert_eq!(
            runs_text(&runs),
            "CIV-0021,MEC-0003,0004 Completed CIV-0050 In Progress"
        );
        assert!(is_app_remark(&runs_text(&runs)));
    }

    #[test]
    fn recognizes_only_the_generated_format() {
        for text in [
            "CIV-0021,0022 Completed CIV-0043 In Progress",
            "CIV-0043 In Progress",
            "CIV-0021 Completed",
        ] {
            assert!(is_app_remark(text), "{text}");
        }
        for text in [
            "",
            "operator note",
            "RFI Status Sync: CIV-1222 In Progress",
            "CIV-0021 Completed (checked by Vali)",
            "civ-0021 Completed",
            "CIV-0021 Done",
        ] {
            assert!(!is_app_remark(text), "{text}");
        }
    }

    #[test]
    fn remark_is_needed_only_for_a_mixed_multi_rfi_cell() {
        assert!(needs_remark(&[vec![Completed, InProgress]]));
        assert!(needs_remark(&[vec![Completed], vec![Pending, Completed]]));
        assert!(!needs_remark(&[vec![Completed, Completed]]));
        assert!(!needs_remark(&[vec![InProgress], vec![Completed]]));
        assert!(!needs_remark(&[]));
    }
}
