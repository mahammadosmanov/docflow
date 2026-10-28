use super::*;

pub(in crate::rfi_follow_up) fn editor_columns() -> Vec<Value> {
    vec![
        json!({
            "key": "enabled",
            "labelKey": "rfi.followUp.column.enabled",
            "kind": "bool",
            "bulk": false,
            "disabledWhen": {"field": "selectable", "equals": false},
        }),
        json!({"key": "rfiNo", "labelKey": "rfi.followUp.column.rfiNo", "kind": "readonly"}),
        json!({"key": "sourceStatuses", "labelKey": "rfi.followUp.column.sourceStatuses", "kind": "readonly"}),
        json!({"key": "currentStatus", "labelKey": "rfi.followUp.column.currentStatus", "kind": "readonly"}),
        json!({"key": "proposedRfiNo", "labelKey": "rfi.followUp.column.proposedRfiNo", "kind": "readonly"}),
        json!({"key": "proposedStatus", "labelKey": "rfi.followUp.column.proposedStatus", "kind": "readonly"}),
        json!({
            "key": "disposition",
            "labelKey": "rfi.followUp.column.disposition",
            "kind": "readonly",
            "display": "localizedKey",
        }),
        json!({
            "key": "reasonKey",
            "labelKey": "rfi.followUp.column.reason",
            "kind": "readonly",
            "display": "localizedKey",
            "detail": true,
        }),
        json!({"key": "mappingDetails", "labelKey": "rfi.followUp.column.mappingDetails", "kind": "readonly", "width": 320, "detail": true}),
        json!({"key": "_sheet", "labelKey": "rfi.followUp.column.sheet", "kind": "readonly", "detail": true}),
        json!({"key": "_row", "labelKey": "rfi.followUp.column.row", "kind": "readonly", "detail": true}),
    ]
}

pub(in crate::rfi_follow_up) fn editor_columns_legacy() -> Vec<Value> {
    editor_columns()
        .into_iter()
        .filter(|column| {
            !matches!(
                column.get("key").and_then(Value::as_str),
                Some("proposedRfiNo" | "mappingDetails")
            )
        })
        .collect()
}

pub(in crate::rfi_follow_up) fn editor_metadata(
    rows: &[FollowUpRow],
    columns: Vec<Value>,
) -> Value {
    json!({
        "kind": "recordTable",
        "layout": "compact",
        "titleKey": "rfi.followUp.editor.title",
        "rowCountKey": "rfi.followUp.editor.rowCount",
        "requiredFields": ["enabled"],
        "groupField": "rfiNo",
        "selectionGroupFields": [],
        "columns": columns,
        "rows": rows,
        "duplicateEnabled": false,
        "selectionEnabled": false,
        "bulkEnabled": false,
        "reorderable": false,
        "detailsEnabled": true,
        "detailsDefaultVisible": rows.iter().any(|row| row.mapping_details.is_some()),
        "copyEnabled": false,
    })
}

pub(in crate::rfi_follow_up) fn borrowed_editor<'a>(
    columns: &'a [Value],
    rows: &'a [BorrowedFollowUpRow<'a>],
) -> BorrowedEditor<'a> {
    BorrowedEditor {
        kind: "recordTable",
        layout: "compact",
        title_key: "rfi.followUp.editor.title",
        row_count_key: "rfi.followUp.editor.rowCount",
        required_fields: ["enabled"],
        group_field: "rfiNo",
        selection_group_fields: [],
        columns,
        rows,
        duplicate_enabled: false,
        selection_enabled: false,
        bulk_enabled: false,
        reorderable: false,
        details_enabled: true,
        details_default_visible: rows.iter().any(|row| row.mapping_details.is_some()),
        copy_enabled: false,
    }
}

pub(in crate::rfi_follow_up) fn validate_projection_budget<T: Serialize>(
    projection: &T,
) -> Result<(), BackendError> {
    let mut writer = CountingLimitWriter {
        count: 0,
        limit: MAX_PREVIEW_AUTHORITY_BYTES,
    };
    serde_json::to_writer(&mut writer, projection)
        .map_err(|_| limit_exceeded_for("the projection is larger than 32 MiB"))
}
