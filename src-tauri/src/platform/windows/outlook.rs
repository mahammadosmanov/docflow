use std::{fs, mem::ManuallyDrop, ptr::null_mut};

use sha2::{Digest, Sha256};
use windows::{
    Win32::System::{
        Com::{
            CLSIDFromProgID, COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
            DISPATCH_FLAGS, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT,
            DISPPARAMS, IDispatch,
        },
        Ole::{DISPID_PROPERTYPUT, GetActiveObject},
        Variant::{VARIANT, VT_BOOL, VT_BSTR, VT_DATE, VT_I4},
    },
    core::{BSTR, GUID, IUnknown, Interface, PCWSTR, w},
};

use crate::outlook_reply_draft::{
    DraftIdentity, MailItemIdentity, OutlookAdapter, OutlookDraft, OutlookFailure, ReplyContent,
    SelectedMailItem,
};

const OUTLOOK_MAIL_ITEM_CLASS: i32 = 43;
const OUTLOOK_PLAIN_TEXT_FORMAT: i32 = 1;
const OUTLOOK_HTML_FORMAT: i32 = 2;
const MAX_COM_TEXT_CHARS: usize = 1_000_000;
const MAX_COM_ID_CHARS: usize = 4096;
const OUTLOOK_MSG_UNICODE_FORMAT: i32 = 9;
const MAX_ARCHIVED_MSG_BYTES: u64 = 100 * 1024 * 1024;
const COMPOUND_FILE_HEADER: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

pub(crate) struct WindowsOutlookAdapter;

impl OutlookAdapter for WindowsOutlookAdapter {
    fn selected_mail_item(&self) -> Result<SelectedMailItem, OutlookFailure> {
        let selected = selected_mail_session()?;
        Ok(SelectedMailItem {
            identity: selected.identity,
            subject: selected.subject,
            fingerprint: draft_fingerprint(&selected.mail)?,
        })
    }

    fn reply_all(
        &self,
        expected: &MailItemIdentity,
    ) -> Result<Box<dyn OutlookDraft>, OutlookFailure> {
        let selected = selected_mail_session()?;
        if &selected.identity != expected {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookDraft.selectionChanged",
            ));
        }
        let reply = dispatch_method(&selected.mail, "ReplyAll", Vec::new())
            .and_then(variant_dispatch)
            .map_err(|_| {
                OutlookFailure::outcome_unknown("error.rfi.outlookDraft.replyAllFailed")
            })?;
        Ok(Box::new(WindowsReplyDraft {
            _apartment: selected._apartment,
            reply,
        }))
    }

    fn delete_unchanged_draft(&self, expected: &DraftIdentity) -> Result<(), OutlookFailure> {
        let _apartment = ComApartment::initialize()?;
        let application = active_outlook_application()?;
        let namespace = property_dispatch(&application, "Session")?;
        let item = dispatch_method(
            &namespace,
            "GetItemFromID",
            vec![
                expected.entry_id.as_str().into(),
                expected.store_id.as_str().into(),
            ],
        )
        .and_then(variant_dispatch)
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.draftMissingOrChanged"))?;
        ensure_mail_item(&item)?;
        let actual = mail_identity(&item)?;
        if actual.entry_id != expected.entry_id || actual.store_id != expected.store_id {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookDraft.draftMissingOrChanged",
            ));
        }
        if !property_bool(&item, "Saved")? {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookDraft.draftMissingOrChanged",
            ));
        }
        let actual_fingerprint = draft_fingerprint(&item)?;
        if actual_fingerprint != expected.fingerprint {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookDraft.draftMissingOrChanged",
            ));
        }
        dispatch_method(&item, "Delete", Vec::new())
            .map_err(|_| OutlookFailure::outcome_unknown("error.rfi.outlookDraft.deleteUnknown"))?;
        Ok(())
    }

    fn export_selected_msg(
        &self,
        expected: &MailItemIdentity,
        expected_fingerprint: &str,
    ) -> Result<Vec<u8>, OutlookFailure> {
        let selected = selected_mail_session()?;
        if &selected.identity != expected
            || draft_fingerprint(&selected.mail)? != expected_fingerprint
        {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookArchive.selectionChanged",
            ));
        }
        let temporary = tempfile::Builder::new()
            .prefix("docflow-outlook-")
            .tempdir()
            .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookArchive.exportFailed"))?;
        let path = temporary.path().join("message.msg");
        let path_text = path.to_string_lossy().into_owned();
        dispatch_method(
            &selected.mail,
            "SaveAs",
            vec![path_text.as_str().into(), OUTLOOK_MSG_UNICODE_FORMAT.into()],
        )
        .map_err(|_| OutlookFailure::outcome_unknown("error.rfi.outlookArchive.exportFailed"))?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| {
            OutlookFailure::outcome_unknown("error.rfi.outlookArchive.exportFailed")
        })?;
        if !metadata.file_type().is_file()
            || metadata.len() < COMPOUND_FILE_HEADER.len() as u64
            || metadata.len() > MAX_ARCHIVED_MSG_BYTES
        {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookArchive.invalidMsg",
            ));
        }
        let bytes = fs::read(&path).map_err(|_| {
            OutlookFailure::outcome_unknown("error.rfi.outlookArchive.exportFailed")
        })?;
        if !bytes.starts_with(COMPOUND_FILE_HEADER) {
            return Err(OutlookFailure::no_effect(
                "error.rfi.outlookArchive.invalidMsg",
            ));
        }
        Ok(bytes)
    }
}

struct WindowsReplyDraft {
    _apartment: ComApartment,
    reply: IDispatch,
}

impl OutlookDraft for WindowsReplyDraft {
    fn prepend_body(&mut self, content: &ReplyContent) -> Result<(), OutlookFailure> {
        let format = property_i32(&self.reply, "BodyFormat").map_err(|_| {
            OutlookFailure::outcome_unknown("error.rfi.outlookDraft.bodyPrependFailed")
        })?;
        match format {
            OUTLOOK_HTML_FORMAT => {
                let existing = bounded_property_string(&self.reply, "HTMLBody").map_err(|_| {
                    OutlookFailure::outcome_unknown("error.rfi.outlookDraft.bodyPrependFailed")
                })?;
                let body = bounded_join(&content.html, &existing)?;
                property_put_string(&self.reply, "HTMLBody", &body).map_err(|_| {
                    OutlookFailure::outcome_unknown("error.rfi.outlookDraft.bodyPrependFailed")
                })?;
            }
            OUTLOOK_PLAIN_TEXT_FORMAT => {
                let existing = bounded_property_string(&self.reply, "Body").map_err(|_| {
                    OutlookFailure::outcome_unknown("error.rfi.outlookDraft.bodyPrependFailed")
                })?;
                let prefix = format!("{}\r\n\r\n", content.plain_text);
                let body = bounded_join(&prefix, &existing)?;
                property_put_string(&self.reply, "Body", &body).map_err(|_| {
                    OutlookFailure::outcome_unknown("error.rfi.outlookDraft.bodyPrependFailed")
                })?;
            }
            _ => {
                return Err(OutlookFailure::outcome_unknown(
                    "error.rfi.outlookDraft.bodyFormatUnsupported",
                ));
            }
        }
        Ok(())
    }

    fn display(&mut self) -> Result<(), OutlookFailure> {
        dispatch_method(&self.reply, "Display", Vec::new()).map_err(|_| {
            OutlookFailure::outcome_unknown("error.rfi.outlookDraft.displayUnknown")
        })?;
        Ok(())
    }

    fn durable_identity(&mut self) -> Option<DraftIdentity> {
        if !property_bool(&self.reply, "Saved").ok()? {
            return None;
        }
        let identity = mail_identity(&self.reply).ok()?;
        let fingerprint = draft_fingerprint(&self.reply).ok()?;
        Some(DraftIdentity {
            entry_id: identity.entry_id,
            store_id: identity.store_id,
            fingerprint,
        })
    }
}

struct SelectedMailSession {
    _apartment: ComApartment,
    _application: IDispatch,
    _explorer: IDispatch,
    _selection: IDispatch,
    mail: IDispatch,
    identity: MailItemIdentity,
    subject: String,
}

fn selected_mail_session() -> Result<SelectedMailSession, OutlookFailure> {
    let apartment = ComApartment::initialize()?;
    let application = active_outlook_application()?;
    let explorer = property_dispatch(&application, "ActiveExplorer")
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.selectionRequired"))?;
    let selection = property_dispatch(&explorer, "Selection")?;
    if property_i32(&selection, "Count")? != 1 {
        return Err(OutlookFailure::no_effect(
            "error.rfi.outlookDraft.singleSelectionRequired",
        ));
    }
    let mail = dispatch_method(&selection, "Item", vec![1_i32.into()])
        .and_then(variant_dispatch)
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.selectionRequired"))?;
    ensure_mail_item(&mail)?;
    let identity = mail_identity(&mail)?;
    let subject = bounded_property_string(&mail, "Subject")?;
    Ok(SelectedMailSession {
        _apartment: apartment,
        _application: application,
        _explorer: explorer,
        _selection: selection,
        mail,
        identity,
        subject,
    })
}

fn ensure_mail_item(item: &IDispatch) -> Result<(), OutlookFailure> {
    if property_i32(item, "Class")? == OUTLOOK_MAIL_ITEM_CLASS {
        Ok(())
    } else {
        Err(OutlookFailure::no_effect(
            "error.rfi.outlookDraft.mailItemRequired",
        ))
    }
}

fn mail_identity(mail: &IDispatch) -> Result<MailItemIdentity, OutlookFailure> {
    let entry_id = bounded_id(property_string(mail, "EntryID")?)?;
    let parent = property_dispatch(mail, "Parent")?;
    let store_id = bounded_id(property_string(&parent, "StoreID")?)?;
    Ok(MailItemIdentity { entry_id, store_id })
}

fn draft_fingerprint(mail: &IDispatch) -> Result<String, OutlookFailure> {
    let mut digest = Sha256::new();
    for property in ["Subject", "Body", "HTMLBody", "To", "CC", "BCC"] {
        let value = bounded_property_string(mail, property)?;
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
    digest.update(property_i32(mail, "Size")?.to_le_bytes());
    digest.update(
        property_date(mail, "LastModificationTime")?
            .to_bits()
            .to_le_bytes(),
    );
    let attachments = property_dispatch(mail, "Attachments")?;
    digest.update(property_i32(&attachments, "Count")?.to_le_bytes());
    Ok(format!("{:x}", digest.finalize()))
}

fn bounded_id(value: String) -> Result<String, OutlookFailure> {
    if !value.is_empty()
        && value.chars().count() <= MAX_COM_ID_CHARS
        && value.chars().all(|character| !character.is_control())
    {
        Ok(value)
    } else {
        Err(OutlookFailure::no_effect(
            "error.rfi.outlookDraft.selectionInvalid",
        ))
    }
}

fn bounded_property_string(
    dispatch: &IDispatch,
    name: &'static str,
) -> Result<String, OutlookFailure> {
    let value = property_string(dispatch, name)?;
    if value.chars().count() <= MAX_COM_TEXT_CHARS {
        Ok(value)
    } else {
        Err(OutlookFailure::no_effect(
            "error.rfi.outlookDraft.contentTooLarge",
        ))
    }
}

fn bounded_join(prefix: &str, existing: &str) -> Result<String, OutlookFailure> {
    if prefix
        .chars()
        .count()
        .saturating_add(existing.chars().count())
        > MAX_COM_TEXT_CHARS
    {
        return Err(OutlookFailure::outcome_unknown(
            "error.rfi.outlookDraft.contentTooLarge",
        ));
    }
    Ok(format!("{prefix}{existing}"))
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, OutlookFailure> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|_| OutlookFailure::unsupported("error.rfi.outlookDraft.comUnavailable"))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn active_outlook_application() -> Result<IDispatch, OutlookFailure> {
    let class = unsafe { CLSIDFromProgID(w!("Outlook.Application")) }
        .map_err(|_| OutlookFailure::unsupported("error.rfi.outlookDraft.outlookUnavailable"))?;
    let mut unknown: Option<IUnknown> = None;
    unsafe { GetActiveObject(&class, None, &mut unknown) }
        .map_err(|_| OutlookFailure::unsupported("error.rfi.outlookDraft.outlookNotRunning"))?;
    unknown
        .ok_or_else(|| OutlookFailure::unsupported("error.rfi.outlookDraft.outlookNotRunning"))?
        .cast::<IDispatch>()
        .map_err(|_| OutlookFailure::unsupported("error.rfi.outlookDraft.outlookUnavailable"))
}

fn property_dispatch(
    dispatch: &IDispatch,
    name: &'static str,
) -> Result<IDispatch, OutlookFailure> {
    property(dispatch, name)
        .and_then(variant_dispatch)
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.comFailed"))
}

fn property_string(dispatch: &IDispatch, name: &'static str) -> Result<String, OutlookFailure> {
    property(dispatch, name)
        .and_then(variant_string)
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.comFailed"))
}

fn property_i32(dispatch: &IDispatch, name: &'static str) -> Result<i32, OutlookFailure> {
    property(dispatch, name)
        .and_then(variant_i32)
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.comFailed"))
}

fn property_bool(dispatch: &IDispatch, name: &'static str) -> Result<bool, OutlookFailure> {
    property(dispatch, name)
        .and_then(variant_bool)
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.comFailed"))
}

fn property_date(dispatch: &IDispatch, name: &'static str) -> Result<f64, OutlookFailure> {
    property(dispatch, name)
        .and_then(variant_date)
        .map_err(|_| OutlookFailure::no_effect("error.rfi.outlookDraft.comFailed"))
}

fn property(dispatch: &IDispatch, name: &'static str) -> windows::core::Result<VARIANT> {
    invoke(dispatch, name, DISPATCH_PROPERTYGET, Vec::new(), false)
}

fn dispatch_method(
    dispatch: &IDispatch,
    name: &'static str,
    arguments: Vec<VARIANT>,
) -> windows::core::Result<VARIANT> {
    invoke(dispatch, name, DISPATCH_METHOD, arguments, false)
}

fn property_put_string(
    dispatch: &IDispatch,
    name: &'static str,
    value: &str,
) -> windows::core::Result<VARIANT> {
    invoke(
        dispatch,
        name,
        DISPATCH_PROPERTYPUT,
        vec![value.into()],
        true,
    )
}

fn invoke(
    dispatch: &IDispatch,
    name: &'static str,
    flags: DISPATCH_FLAGS,
    mut arguments: Vec<VARIANT>,
    property_put: bool,
) -> windows::core::Result<VARIANT> {
    arguments.reverse();
    let wide = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let name_pointer = PCWSTR(wide.as_ptr());
    let iid_null = GUID::from_u128(0);
    let mut dispatch_id = 0;
    unsafe { dispatch.GetIDsOfNames(&iid_null, &name_pointer, 1, 0, &mut dispatch_id) }?;

    let mut named = if property_put {
        vec![DISPID_PROPERTYPUT]
    } else {
        Vec::new()
    };
    let parameters = DISPPARAMS {
        rgvarg: if arguments.is_empty() {
            null_mut()
        } else {
            arguments.as_mut_ptr()
        },
        rgdispidNamedArgs: if named.is_empty() {
            null_mut()
        } else {
            named.as_mut_ptr()
        },
        cArgs: u32::try_from(arguments.len()).unwrap_or(u32::MAX),
        cNamedArgs: u32::try_from(named.len()).unwrap_or(u32::MAX),
    };
    let mut result = VARIANT::default();
    unsafe {
        dispatch.Invoke(
            dispatch_id,
            &iid_null,
            0,
            flags,
            &parameters,
            Some(&mut result),
            None,
            None,
        )
    }?;
    Ok(result)
}

fn variant_dispatch(value: VARIANT) -> windows::core::Result<IDispatch> {
    IDispatch::try_from(&value)
}

fn variant_string(value: VARIANT) -> windows::core::Result<String> {
    if value.vt() != VT_BSTR {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x8002_0005_u32 as i32,
        )));
    }
    let bstr = unsafe {
        &*(&value.Anonymous.Anonymous.Anonymous.bstrVal as *const ManuallyDrop<BSTR> as *const BSTR)
    };
    String::from_utf16(bstr).map_err(|_| {
        windows::core::Error::from_hresult(windows::core::HRESULT(0x8002_0005_u32 as i32))
    })
}

fn variant_i32(value: VARIANT) -> windows::core::Result<i32> {
    if value.vt() != VT_I4 {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x8002_0005_u32 as i32,
        )));
    }
    Ok(unsafe { value.Anonymous.Anonymous.Anonymous.lVal })
}

fn variant_bool(value: VARIANT) -> windows::core::Result<bool> {
    if value.vt() != VT_BOOL {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x8002_0005_u32 as i32,
        )));
    }
    Ok(unsafe { value.Anonymous.Anonymous.Anonymous.boolVal.0 != 0 })
}

fn variant_date(value: VARIANT) -> windows::core::Result<f64> {
    if value.vt() != VT_DATE {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x8002_0005_u32 as i32,
        )));
    }
    Ok(unsafe { value.Anonymous.Anonymous.Anonymous.date })
}
