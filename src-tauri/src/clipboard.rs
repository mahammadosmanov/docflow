use std::io;

use crate::{errors::BackendError, rfi_log::ClipboardPayload};

const MAX_CLIPBOARD_BYTES: usize = 8 * 1024 * 1024;

pub fn write(payload: &ClipboardPayload) -> Result<(), BackendError> {
    if payload.html.len() > MAX_CLIPBOARD_BYTES || payload.plain.len() > MAX_CLIPBOARD_BYTES {
        return Err(BackendError::validation(
            "clipboard table exceeded the size limit",
        ));
    }
    write_platform(payload).map_err(|_| BackendError::request_failed("clipboard write failed"))
}

pub fn cf_html(fragment: &str) -> Result<Vec<u8>, BackendError> {
    if fragment.len() > MAX_CLIPBOARD_BYTES {
        return Err(BackendError::validation(
            "clipboard table exceeded the size limit",
        ));
    }
    let document = format!(
        "<html><head><meta charset=\"utf-8\"></head><body><!--StartFragment-->{fragment}<!--EndFragment--></body></html>"
    );
    let header_template = |start_html: usize,
                           end_html: usize,
                           start_fragment: usize,
                           end_fragment: usize| {
        format!(
            "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
        )
    };
    let placeholder = header_template(0, 0, 0, 0);
    let start_html = placeholder.len();
    let start_marker = "<!--StartFragment-->";
    let end_marker = "<!--EndFragment-->";
    let start_fragment = start_html
        + document
            .find(start_marker)
            .ok_or_else(|| BackendError::protocol("HTML fragment marker is missing"))?
        + start_marker.len();
    let end_fragment = start_html
        + document
            .find(end_marker)
            .ok_or_else(|| BackendError::protocol("HTML fragment marker is missing"))?;
    let end_html = start_html + document.len();
    let mut result =
        header_template(start_html, end_html, start_fragment, end_fragment).into_bytes();
    result.extend_from_slice(document.as_bytes());
    result.push(0);
    Ok(result)
}

#[cfg(target_os = "macos")]
fn write_platform(payload: &ClipboardPayload) -> io::Result<()> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "clipboard input is unavailable"))?
        .write_all(payload.plain.as_bytes())?;
    if child.wait()?.success() {
        Ok(())
    } else {
        Err(io::Error::other("clipboard process failed"))
    }
}

#[cfg(target_os = "windows")]
fn write_platform(payload: &ClipboardPayload) -> io::Result<()> {
    use std::{ptr, thread, time::Duration};
    use windows_sys::Win32::System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW},
        Ole::CF_UNICODETEXT,
    };

    let html = cf_html(&payload.html).map_err(|error| io::Error::other(error.message))?;
    let mut plain = payload.plain.encode_utf16().collect::<Vec<_>>();
    plain.push(0);
    let plain = plain
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let format = "HTML Format\0".encode_utf16().collect::<Vec<_>>();

    let mut opened = false;
    for _ in 0..10 {
        if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        thread::sleep(Duration::from_millis(40));
    }
    if !opened {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        if unsafe { EmptyClipboard() } == 0 {
            return Err(io::Error::last_os_error());
        }
        let html_format = unsafe { RegisterClipboardFormatW(format.as_ptr()) };
        if html_format == 0 {
            return Err(io::Error::last_os_error());
        }
        put_windows_clipboard(html_format, &html)?;
        put_windows_clipboard(u32::from(CF_UNICODETEXT), &plain)
    })();
    unsafe { CloseClipboard() };
    result
}

#[cfg(target_os = "windows")]
fn put_windows_clipboard(format: u32, data: &[u8]) -> io::Result<()> {
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{
            DataExchange::SetClipboardData,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        },
    };

    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, data.len()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        unsafe { GlobalFree(handle) };
        return Err(io::Error::last_os_error());
    }
    unsafe {
        ptr::copy_nonoverlapping(data.as_ptr(), pointer.cast::<u8>(), data.len());
        GlobalUnlock(handle);
    }
    if unsafe { SetClipboardData(format, handle) }.is_null() {
        unsafe { GlobalFree(handle) };
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn write_platform(_payload: &ClipboardPayload) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "platform clipboard is unsupported",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cf_html_offsets_are_utf8_byte_offsets() {
        let payload = cf_html("<b>Şimal &amp; Cənub</b>").unwrap();
        let text = String::from_utf8(payload[..payload.len() - 1].to_vec()).unwrap();
        let offset = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}:")))
                .unwrap()
                .parse::<usize>()
                .unwrap()
        };
        let start = offset("StartFragment");
        let end = offset("EndFragment");
        assert_eq!(
            &text.as_bytes()[start..end],
            "<b>Şimal &amp; Cənub</b>".as_bytes()
        );
        assert_eq!(offset("EndHTML"), text.len());
    }
}
