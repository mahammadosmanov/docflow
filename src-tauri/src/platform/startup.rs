use std::io;

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS, ERROR_UNSUPPORTED_TYPE},
    System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    },
};

#[cfg(any(target_os = "windows", test))]
const MAX_RUN_COMMAND_UNITS: usize = 260;

#[cfg(target_os = "windows")]
pub(crate) fn set_start_with_windows(enabled: bool) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let subkey = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let command = if enabled {
        let executable = std::env::current_exe()?;
        Some(startup_command(executable.as_os_str().encode_wide())?)
    } else {
        None
    };
    set_run_entry(&subkey, command.as_deref())
}

#[cfg(any(target_os = "windows", test))]
fn startup_command(path: impl Iterator<Item = u16>) -> io::Result<Vec<u16>> {
    let quote = u16::from(b'"');
    let mut command = vec![quote];
    for unit in path {
        if unit == 0 || unit == quote || command.len() >= MAX_RUN_COMMAND_UNITS - 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid or oversized Windows startup command",
            ));
        }
        command.push(unit);
    }
    command.extend([quote, 0]);
    Ok(command)
}

#[cfg(target_os = "windows")]
fn set_run_entry(subkey: &[u16], command: Option<&[u16]>) -> io::Result<()> {
    let value_name = wide("DocFlow");
    let status = if let Some(command) = command {
        if run_entry_matches(subkey, &value_name, command)? {
            // Windows runs this executable from the Run key too. Avoid a write on normal logon.
            return Ok(());
        }
        // SAFETY: subkey, name and command are NUL-terminated and live throughout the call.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                value_name.as_ptr(),
                REG_SZ,
                command.as_ptr().cast(),
                std::mem::size_of_val(command) as u32,
            )
        }
    } else {
        // SAFETY: subkey and name are NUL-terminated and live throughout the call.
        let status =
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), value_name.as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        status
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

#[cfg(target_os = "windows")]
fn run_entry_matches(subkey: &[u16], name: &[u16], command: &[u16]) -> io::Result<bool> {
    let mut stored = [0u16; MAX_RUN_COMMAND_UNITS + 1];
    let mut bytes = std::mem::size_of_val(&stored) as u32;
    // SAFETY: subkey and name are NUL-terminated; stored is writable for the supplied byte count.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            stored.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    match status {
        ERROR_SUCCESS => Ok(bytes as usize == std::mem::size_of_val(command)
            && stored.get(..command.len()) == Some(command)),
        ERROR_FILE_NOT_FOUND | ERROR_MORE_DATA | ERROR_UNSUPPORTED_TYPE => Ok(false),
        other => Err(io::Error::from_raw_os_error(other as i32)),
    }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn set_start_with_windows(enabled: bool) -> io::Result<()> {
    if enabled {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows startup is available only on Windows",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn repairs_missing_and_stale_run_entries_and_removes_on_disable() {
        use windows_sys::Win32::System::Registry::RegDeleteKeyW;

        struct TestKey(Vec<u16>);

        impl Drop for TestKey {
            fn drop(&mut self) {
                // SAFETY: this is the test's unique disposable NUL-terminated subkey.
                unsafe {
                    RegDeleteKeyW(HKEY_CURRENT_USER, self.0.as_ptr());
                }
            }
        }

        let key = TestKey(wide(&format!(
            r"Software\DocFlowStartupTest-{}",
            uuid::Uuid::new_v4(),
        )));
        let name = wide("DocFlow");
        let expected =
            startup_command(r"C:\Program Files\DocFlow\docflow.exe".encode_utf16()).unwrap();
        let stale = startup_command(r"C:\Previous Location\docflow.exe".encode_utf16()).unwrap();

        assert!(!run_entry_matches(&key.0, &name, &expected).unwrap());
        set_run_entry(&key.0, Some(&expected)).unwrap();
        assert!(run_entry_matches(&key.0, &name, &expected).unwrap());

        set_run_entry(&key.0, Some(&stale)).unwrap();
        assert!(!run_entry_matches(&key.0, &name, &expected).unwrap());
        set_run_entry(&key.0, Some(&expected)).unwrap();
        assert!(run_entry_matches(&key.0, &name, &expected).unwrap());

        set_run_entry(&key.0, None).unwrap();
        set_run_entry(&key.0, None).unwrap();
        assert!(!run_entry_matches(&key.0, &name, &expected).unwrap());
    }

    #[test]
    fn startup_command_preserves_utf16_and_quotes_spaces() {
        let mut path: Vec<_> = "C:\\Program Files\\DocFlow\\".encode_utf16().collect();
        path.push(0xD800);
        path.extend("docflow.exe".encode_utf16());
        let mut expected = vec![b'"' as u16];
        expected.extend_from_slice(&path);
        expected.extend([b'"' as u16, 0]);

        assert_eq!(startup_command(path.into_iter()).unwrap(), expected);
    }

    #[test]
    fn startup_command_rejects_invalid_or_oversized_paths() {
        let max_path = vec![b'x' as u16; 258];
        assert!(startup_command(max_path.into_iter()).is_ok());
        for path in [vec![b'x' as u16; 259], vec![b'"' as u16], vec![0]] {
            assert_eq!(
                startup_command(path.into_iter()).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
