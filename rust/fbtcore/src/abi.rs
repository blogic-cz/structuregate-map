//! The two string crossings every entry point makes: a caller's string in, and an answer out that the
//! caller frees with `fbt_string_free`. Out of `lib.rs` because that file is baselined and every new
//! half's `mod` line has to be paid for there.

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Library version, for a caller that wants to check what it linked.
///
/// The returned pointer is static and must NOT be freed.
#[unsafe(no_mangle)]
pub extern "C" fn fbt_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Turn a Rust string into one the caller must free with `fbt_string_free`.
pub(crate) fn out_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        // A NUL inside means the payload was not text this ABI can carry. Say so
        // rather than truncating at the NUL and handing back a half answer.
        Err(_) => CString::new(r#"{"error":"result contained a NUL byte"}"#)
            .expect("literal has no NUL")
            .into_raw(),
    }
}

/// Read a caller's string. `None` when the pointer is null or not UTF-8.
///
/// # Safety
/// `p` must be null or a NUL-terminated string that stays alive for this call.
pub(crate) unsafe fn in_string(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(p) }.to_str().ok().map(|s| s.to_string())
}

/// Free a string this library returned. Safe to call with null.
///
/// # Safety
/// `s` must be null or a pointer returned by this library and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fbt_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        drop(unsafe { CString::from_raw(s) });
    }));
}
