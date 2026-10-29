//! Reading registry values.

use std::ffi::c_void;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, KEY_READ, REG_ROUTINE_FLAGS, RRF_RT_ANY, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};

use super::{from_wide, wide};

/// Read value `name` under `key` into `data` if its type is one `flags` allows; the number
/// of bytes read, or `None` when there is no such value. A null `data` only checks.
///
/// # Safety
/// `data` is null or writable for `size` bytes.
unsafe fn get_value(
    root: HKEY,
    key: &str,
    name: &str,
    flags: REG_ROUTINE_FLAGS,
    data: *mut c_void,
    size: u32,
) -> Option<u32> {
    let (key, name) = (wide(key), wide(name));
    let mut size = size;
    let size_ptr = if data.is_null() {
        null_mut()
    } else {
        &raw mut size
    };
    // SAFETY: NUL-terminated names; the caller vouches for `data` and `size`.
    let r = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            name.as_ptr(),
            flags,
            null_mut(),
            data,
            size_ptr,
        )
    };
    (r == ERROR_SUCCESS).then_some(size)
}

/// Whether value `name` exists under `key`.
pub(super) fn exists(root: HKEY, key: &str, name: &str) -> bool {
    // SAFETY: no data is read.
    unsafe { get_value(root, key, name, RRF_RT_ANY, null_mut(), 0) }.is_some()
}

/// A `REG_DWORD` value.
pub(super) fn get_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let mut data = 0u32;
    // SAFETY: `data` holds 4 bytes.
    unsafe {
        get_value(
            root,
            key,
            name,
            RRF_RT_REG_DWORD,
            (&raw mut data).cast(),
            size_of::<u32>() as u32,
        )
    }?;
    Some(data)
}

/// A `REG_SZ` value, however long: its size is asked first.
pub(super) fn get_sz(root: HKEY, key: &str, name: &str) -> Option<String> {
    let (key, name) = (wide(key), wide(name));
    // In bytes; the value may grow between the two calls, then it is asked again.
    let read = |data: *mut c_void, size: &mut u32| {
        // SAFETY: NUL-terminated names; `data` is null (a size query) or holds `size` bytes,
        // as the callers below see to.
        unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                data,
                size,
            )
        }
    };
    loop {
        let mut size = 0u32;
        if read(null_mut(), &mut size) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(size_of::<u16>()).max(1)];
        let mut size = (buf.len() * size_of::<u16>()) as u32;
        match read(buf.as_mut_ptr().cast(), &mut size) {
            ERROR_SUCCESS => return Some(from_wide(&buf)),
            ERROR_MORE_DATA => {}
            _ => return None,
        }
    }
}

/// The names of the subkeys of `key` (none when it does not exist).
pub(super) fn subkeys(root: HKEY, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut handle: HKEY = null_mut();
    let path = wide(key);
    // SAFETY: NUL-terminated name and a valid out-pointer; the key is closed below.
    if unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ, &mut handle) } != ERROR_SUCCESS {
        return out;
    }
    for i in 0.. {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY: open key; `name` holds `len` characters.
        let r = unsafe {
            RegEnumKeyExW(
                handle,
                i,
                name.as_mut_ptr(),
                &mut len,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if r != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    // SAFETY: the key opened above.
    unsafe { RegCloseKey(handle) };
    out
}

/// A `REG_BINARY` value read into `buf`; the number of bytes read.
pub(super) fn get_binary(root: HKEY, key: &str, name: &str, buf: &mut [u8]) -> Option<usize> {
    // SAFETY: `buf` holds its length in bytes.
    let n = unsafe {
        get_value(
            root,
            key,
            name,
            RRF_RT_REG_BINARY,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
        )
    }?;
    Some(n as usize)
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

    use super::*;

    #[test]
    fn strings() {
        const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let name = get_sz(HKEY_LOCAL_MACHINE, KEY, "ProductName");
        assert!(name.is_some_and(|n| n.starts_with("Windows")));
        assert_eq!(get_sz(HKEY_LOCAL_MACHINE, KEY, "NoSuchValue"), None);
        assert_eq!(
            get_sz(HKEY_LOCAL_MACHINE, KEY, "InstallDate"),
            None,
            "a DWORD"
        );
    }
}
