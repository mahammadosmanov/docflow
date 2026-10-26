use super::*;

fn update(expected: &str, replacement: &str) -> GuardedCellUpdateDto {
    GuardedCellUpdateDto {
        row_id: "r1".to_string(),
        expected_value: expected.to_string(),
        expected_formula: None,
        expected_style: None,
        replacement_value: replacement.to_string(),
        row: 5,
        col: 9,
    }
}

#[test]
fn publish_kind_authorizes_only_blank_to_ongoing() {
    let kind = SyncWorkbookKind::RfiLogPublish;
    assert!(revise::validate_replacement(kind, &update("", "Ongoing")));
    assert!(revise::validate_replacement(kind, &update("  ", "Ongoing")));
    for expected in ["Ongoing", "Completed", "Rejected"] {
        assert!(
            !revise::validate_replacement(kind, &update(expected, "Ongoing")),
            "{expected} must not be overwritten"
        );
    }
    for replacement in ["Completed", "ongoing", "", "Ongoing "] {
        assert!(
            !revise::validate_replacement(kind, &update("", replacement)),
            "{replacement:?} must be rejected"
        );
    }
}

#[test]
fn completion_kind_behavior_is_unchanged() {
    let kind = SyncWorkbookKind::RfiLogStatus;
    assert!(revise::validate_replacement(
        kind,
        &update("Ongoing", "Completed")
    ));
    assert!(!revise::validate_replacement(kind, &update("", "Ongoing")));
}
