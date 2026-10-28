use super::*;

#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub(crate) sheet: &'static str,
    pub(crate) issue_col: u32,
    pub(crate) inspection_col: u32,
    pub(crate) expiry_col: u32,
    pub(crate) time_col: u32,
    pub(crate) azfen_col: u32,
    pub(crate) intec_col: u32,
    pub(crate) location_col: u32,
    pub(crate) subcontractor_col: u32,
    pub(crate) status_col: u32,
    pub(crate) remarks_col: u32,
    pub(crate) validity_col: u32,
    pub(crate) row_col: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RfiStatusColumns {
    pub sheet: &'static str,
    pub rfi_column: u32,
    pub status_column: u32,
}

pub(crate) fn status_columns() -> [RfiStatusColumns; 4] {
    DISCIPLINES.map(|discipline| {
        let projection = layout(discipline).expect("canonical RFI Log discipline has a layout");
        RfiStatusColumns {
            sheet: projection.sheet,
            rfi_column: 1,
            status_column: projection.remarks_col,
        }
    })
}

pub(crate) fn layout(discipline: &str) -> Option<Layout> {
    match discipline {
        "CIV" => Some(Layout {
            sheet: "CIV",
            issue_col: 8,
            inspection_col: 9,
            expiry_col: 10,
            time_col: 11,
            azfen_col: 12,
            intec_col: 13,
            location_col: 14,
            subcontractor_col: 15,
            status_col: 16,
            remarks_col: 20,
            validity_col: 17,
            row_col: None,
        }),
        "MEC" => Some(Layout {
            sheet: "MEC",
            issue_col: 9,
            inspection_col: 10,
            expiry_col: 11,
            time_col: 12,
            azfen_col: 13,
            intec_col: 14,
            location_col: 15,
            subcontractor_col: 16,
            status_col: 17,
            remarks_col: 21,
            validity_col: 18,
            row_col: Some(8),
        }),
        "ELE" => Some(Layout {
            sheet: "ELE",
            issue_col: 8,
            inspection_col: 9,
            expiry_col: 10,
            time_col: 11,
            azfen_col: 12,
            intec_col: 13,
            location_col: 14,
            subcontractor_col: 15,
            status_col: 16,
            remarks_col: 20,
            validity_col: 17,
            row_col: None,
        }),
        "PRC" => Some(Layout {
            sheet: "PRC",
            issue_col: 8,
            inspection_col: 9,
            expiry_col: 10,
            time_col: 11,
            azfen_col: 12,
            intec_col: 13,
            location_col: 14,
            subcontractor_col: 15,
            status_col: 16,
            remarks_col: 20,
            validity_col: 17,
            row_col: None,
        }),
        _ => None,
    }
}
