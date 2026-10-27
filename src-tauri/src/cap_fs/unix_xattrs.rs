use std::{fs::File, io};

use super::*;

#[cfg(unix)]
pub(super) fn capture_unix_xattrs(file: &File) -> io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
    use std::os::fd::AsRawFd;

    let descriptor = file.as_raw_fd();
    #[cfg(target_os = "macos")]
    // SAFETY: descriptor is the live fd of file; a null list with size 0 only queries the required length.
    let list_size = unsafe { libc::flistxattr(descriptor, std::ptr::null_mut(), 0, 0) };
    #[cfg(not(target_os = "macos"))]
    // SAFETY: descriptor is the live fd of file; a null list with size 0 only queries the required length.
    let list_size = unsafe { libc::flistxattr(descriptor, std::ptr::null_mut(), 0) };
    if list_size < 0 {
        return Err(io::Error::last_os_error());
    }
    if u64::try_from(list_size).unwrap_or(u64::MAX) > MAX_SECURITY_AUXILIARY_BYTES {
        return Err(io::Error::other(
            "file auxiliary metadata exceeded the size limit",
        ));
    }
    let mut names = vec![0_u8; list_size as usize];
    if list_size > 0 {
        #[cfg(target_os = "macos")]
        // SAFETY: names holds names.len() bytes, the length reported by the size query above.
        let read =
            unsafe { libc::flistxattr(descriptor, names.as_mut_ptr().cast(), names.len(), 0) };
        #[cfg(not(target_os = "macos"))]
        // SAFETY: names holds names.len() bytes, the length reported by the size query above.
        let read = unsafe { libc::flistxattr(descriptor, names.as_mut_ptr().cast(), names.len()) };
        if read != list_size {
            return Err(io::Error::other(
                "file auxiliary metadata changed during capture",
            ));
        }
    }
    let mut records = names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(<[u8]>::to_vec)
        .collect::<Vec<_>>();
    records.sort();
    records.dedup();
    if records.len() > MAX_SECURITY_AUXILIARY_RECORDS {
        return Err(io::Error::other(
            "file auxiliary metadata record count exceeded the limit",
        ));
    }

    let mut total = 0_u64;
    let mut output = Vec::with_capacity(records.len());
    for name in records {
        let c_name = std::ffi::CString::new(name.clone())
            .map_err(|_| io::Error::other("file auxiliary metadata name is invalid"))?;
        #[cfg(target_os = "macos")]
        // SAFETY: descriptor is live and c_name is NUL-terminated; a null buffer with size 0 only queries the value length.
        let value_size =
            unsafe { libc::fgetxattr(descriptor, c_name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
        #[cfg(not(target_os = "macos"))]
        // SAFETY: descriptor is live and c_name is NUL-terminated; a null buffer with size 0 only queries the value length.
        let value_size =
            unsafe { libc::fgetxattr(descriptor, c_name.as_ptr(), std::ptr::null_mut(), 0) };
        if value_size < 0 {
            return Err(io::Error::last_os_error());
        }
        total = total
            .checked_add(name.len() as u64)
            .and_then(|value| value.checked_add(value_size as u64))
            .and_then(|value| value.checked_add(16))
            .filter(|value| *value <= MAX_SECURITY_AUXILIARY_BYTES)
            .ok_or_else(|| io::Error::other("file auxiliary metadata exceeded the size limit"))?;
        let mut value = vec![0_u8; value_size as usize];
        if value_size > 0 {
            #[cfg(target_os = "macos")]
            // SAFETY: value holds value.len() bytes, the length from the size query, and c_name is NUL-terminated.
            let read = unsafe {
                libc::fgetxattr(
                    descriptor,
                    c_name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                    0,
                    0,
                )
            };
            #[cfg(not(target_os = "macos"))]
            // SAFETY: value holds value.len() bytes, the length from the size query, and c_name is NUL-terminated.
            let read = unsafe {
                libc::fgetxattr(
                    descriptor,
                    c_name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                )
            };
            if read != value_size {
                return Err(io::Error::other(
                    "file auxiliary metadata changed during capture",
                ));
            }
        }
        output.push((name, value));
    }
    Ok(output)
}

#[cfg(unix)]
pub(super) fn replay_unix_xattrs(file: &File, records: &[(Vec<u8>, Vec<u8>)]) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    if records.len() > MAX_SECURITY_AUXILIARY_RECORDS {
        return Err(io::Error::other(
            "file auxiliary metadata record count exceeded the limit",
        ));
    }
    for (name, value) in records {
        let c_name = std::ffi::CString::new(name.clone())
            .map_err(|_| io::Error::other("file auxiliary metadata name is invalid"))?;
        #[cfg(target_os = "macos")]
        // SAFETY: the descriptor is live, c_name is NUL-terminated, and value is a live slice of value.len() bytes.
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c_name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        #[cfg(not(target_os = "macos"))]
        // SAFETY: the descriptor is live, c_name is NUL-terminated, and value is a live slice of value.len() bytes.
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c_name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
