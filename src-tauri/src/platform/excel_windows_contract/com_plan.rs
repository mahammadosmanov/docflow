use super::MAX_DISPATCH_STRING_CHARS;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ComValue {
    I32(i32),
    Bool(bool),
    String(String),
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DispatchInvokeKind {
    Method,
    PropertyGet,
    PropertyPut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EncodedVariant {
    I32(i32),
    Bool(bool),
    String(String),
    Error(i32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EncodedDispatchParameters {
    pub(crate) flags: u16,
    pub(crate) arguments: Vec<EncodedVariant>,
    pub(crate) named_dispids: Vec<i32>,
}

pub(crate) fn encode_dispatch_parameters(
    values: &[ComValue],
    kind: DispatchInvokeKind,
) -> EncodedDispatchParameters {
    const DISP_E_PARAMNOTFOUND_VALUE: i32 = i32::from_ne_bytes(0x8002_0004_u32.to_ne_bytes());
    let arguments = values
        .iter()
        .map(|value| match value {
            ComValue::I32(value) => EncodedVariant::I32(*value),
            ComValue::Bool(value) => EncodedVariant::Bool(*value),
            ComValue::String(value) => EncodedVariant::String(value.clone()),
            ComValue::Missing => EncodedVariant::Error(DISP_E_PARAMNOTFOUND_VALUE),
        })
        .collect();
    let (flags, named_dispids) = match kind {
        DispatchInvokeKind::Method => (1, Vec::new()),
        DispatchInvokeKind::PropertyGet => (2, Vec::new()),
        DispatchInvokeKind::PropertyPut => (4, vec![-3]),
    };
    EncodedDispatchParameters {
        flags,
        arguments,
        named_dispids,
    }
}

impl From<i32> for ComValue {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}

impl From<bool> for ComValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ComOperation {
    InitializeSta,
    CreateApplication,
    SetProperty {
        name: &'static str,
        value: ComValue,
        named_property_put: bool,
    },
    OpenWorkbook {
        arguments: Vec<ComValue>,
    },
    ResolveWorksheet {
        arguments: Vec<ComValue>,
    },
    VerifyWorksheet {
        expected_name: String,
        expected_visibility: i32,
    },
    ExportWorksheetPdf {
        arguments: Vec<ComValue>,
    },
    CloseWorkbook {
        arguments: Vec<ComValue>,
    },
    QuitApplication,
    ReleaseObject(ComObject),
    UninitializeSta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ComObject {
    Application,
    Workbooks,
    Workbook,
    Worksheets,
    Worksheet,
}

pub(crate) struct ComPlanRequest {
    pub(crate) workbook_path: String,
    pub(crate) pdf_path: String,
    pub(crate) workbook_index: u32,
    pub(crate) worksheet_name: String,
    pub(crate) worksheet_visibility: crate::ooxml::WorksheetVisibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ComPlanError;

pub(crate) fn validate_dispatch_member(name: &str) -> Result<(), ComPlanError> {
    match name {
        "AutomationSecurity"
        | "EnableEvents"
        | "DisplayAlerts"
        | "AskToUpdateLinks"
        | "Visible"
        | "Workbooks"
        | "Open"
        | "Worksheets"
        | "Item"
        | "Name"
        | "ExportAsFixedFormat"
        | "Close"
        | "Quit" => Ok(()),
        _ => Err(ComPlanError),
    }
}

pub(crate) fn build_com_operation_plan(
    request: &ComPlanRequest,
) -> Result<Vec<ComOperation>, ComPlanError> {
    if request.workbook_index == 0
        || request.workbook_index > 250
        || !is_bounded_dispatch_string(&request.workbook_path)
        || !is_bounded_dispatch_string(&request.pdf_path)
        || !is_bounded_dispatch_string(&request.worksheet_name)
    {
        return Err(ComPlanError);
    }
    Ok(vec![
        ComOperation::InitializeSta,
        ComOperation::CreateApplication,
        set_property("AutomationSecurity", 3.into()),
        set_property("EnableEvents", false.into()),
        set_property("DisplayAlerts", false.into()),
        set_property("AskToUpdateLinks", false.into()),
        set_property("Visible", false.into()),
        ComOperation::OpenWorkbook {
            arguments: vec![
                ComValue::I32(0),
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Missing,
                ComValue::Missing,
                ComValue::Missing,
                ComValue::Bool(true),
                ComValue::String(String::new()),
                ComValue::String(String::new()),
                ComValue::Missing,
                ComValue::Bool(true),
                ComValue::I32(0),
                ComValue::String(request.workbook_path.clone()),
            ],
        },
        ComOperation::ResolveWorksheet {
            arguments: vec![ComValue::I32(request.workbook_index as i32)],
        },
        ComOperation::VerifyWorksheet {
            expected_name: request.worksheet_name.clone(),
            expected_visibility: worksheet_visibility_value(request.worksheet_visibility),
        },
        ComOperation::ExportWorksheetPdf {
            arguments: vec![
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Missing,
                ComValue::Missing,
                ComValue::Bool(false),
                ComValue::Bool(true),
                ComValue::I32(0),
                ComValue::String(request.pdf_path.clone()),
                ComValue::I32(0),
            ],
        },
        ComOperation::CloseWorkbook {
            arguments: vec![ComValue::Bool(false)],
        },
        ComOperation::QuitApplication,
        ComOperation::ReleaseObject(ComObject::Worksheet),
        ComOperation::ReleaseObject(ComObject::Worksheets),
        ComOperation::ReleaseObject(ComObject::Workbook),
        ComOperation::ReleaseObject(ComObject::Workbooks),
        ComOperation::ReleaseObject(ComObject::Application),
        ComOperation::UninitializeSta,
    ])
}

fn is_bounded_dispatch_string(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\0')
        && value.encode_utf16().count() <= MAX_DISPATCH_STRING_CHARS
}

fn worksheet_visibility_value(visibility: crate::ooxml::WorksheetVisibility) -> i32 {
    match visibility {
        crate::ooxml::WorksheetVisibility::Visible => -1,
        crate::ooxml::WorksheetVisibility::Hidden => 0,
        crate::ooxml::WorksheetVisibility::VeryHidden => 2,
    }
}

pub(super) fn set_property(name: &'static str, value: ComValue) -> ComOperation {
    ComOperation::SetProperty {
        name,
        value,
        named_property_put: true,
    }
}
