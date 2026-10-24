//! Protection of secrets at rest.
//!
//! On Windows, data is encrypted with DPAPI (`CryptProtectData`), bound to
//! the current Windows user. On other platforms there is no equivalent in
//! std, so `protect` refuses and callers keep secrets in memory only.

use crate::error::{Error, Result};

pub fn is_supported() -> bool {
    cfg!(windows)
}

pub fn protect(plain: &[u8]) -> Result<Vec<u8>> {
    imp::protect(plain)
}

pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>> {
    imp::unprotect(cipher)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    const ENTROPY: &[u8] = b"kestrel-launcher-accounts-v1";

    fn blob(data: &[u8]) -> Result<CRYPT_INTEGER_BLOB> {
        Ok(CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len())
                .map_err(|_| Error::Other("secret too large".into()))?,
            // DPAPI does not write through pDataIn/pOptionalEntropy.
            pbData: data.as_ptr() as *mut u8,
        })
    }

    /// Copy the DPAPI-allocated output and free it.
    ///
    /// # Safety
    /// `out` must have been filled by a successful CryptProtectData /
    /// CryptUnprotectData call (LocalAlloc'd buffer of `cbData` bytes).
    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let v = if out.pbData.is_null() || out.cbData == 0 {
            Vec::new()
        } else {
            // SAFETY: per the caller contract, pbData points to cbData
            // initialised bytes owned by us until LocalFree below.
            unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec()
        };
        if !out.pbData.is_null() {
            // SAFETY: the buffer was allocated by DPAPI with LocalAlloc and is
            // freed exactly once; the slice above was copied before this.
            unsafe { LocalFree(out.pbData as _) };
        }
        v
    }

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>> {
        let input = blob(plain)?;
        let entropy = blob(ENTROPY)?;
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: input/entropy point to live slices for the whole call;
        // `out` is a valid out-pointer; null description/prompt are allowed.
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(Error::Other(format!(
                "CryptProtectData failed: {}",
                std::io::Error::last_os_error()
            )));
        }
        // SAFETY: the call succeeded, so `out` holds a LocalAlloc'd buffer.
        Ok(unsafe { take(out) })
    }

    pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>> {
        let input = blob(cipher)?;
        let entropy = blob(ENTROPY)?;
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: as in `protect`; a null description out-pointer is allowed.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(Error::Other(format!(
                "CryptUnprotectData failed: {}",
                std::io::Error::last_os_error()
            )));
        }
        // SAFETY: the call succeeded, so `out` holds a LocalAlloc'd buffer.
        Ok(unsafe { take(out) })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn protect(_plain: &[u8]) -> Result<Vec<u8>> {
        Err(Error::Other(
            "secure storage is only implemented on Windows".into(),
        ))
    }

    pub fn unprotect(_cipher: &[u8]) -> Result<Vec<u8>> {
        Err(Error::Other(
            "secure storage is only implemented on Windows".into(),
        ))
    }
}
