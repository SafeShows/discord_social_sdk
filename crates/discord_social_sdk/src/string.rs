//! Conversions across the `Discord_String` boundary.
//!
//! The SDK uses two different ownership rules depending on direction:
//!
//! - **Into the SDK** (setters, arguments): the SDK copies immediately, so a
//!   `Discord_String` borrowing Rust memory is sound as long as it only has to
//!   survive the call. That is [`borrow`].
//! - **Out of the SDK** (getters, callback payloads): the returned buffer is
//!   freshly allocated and the caller owns it. It must be released with
//!   `Discord_Free`. That is [`take`].
//!
//! `Discord_String` is *not* NUL-terminated, so it is always a `ptr`/`size`
//! pair and never a `CStr`.

use discord_social_sdk_sys as sys;
use std::mem::MaybeUninit;

/// Borrow a Rust string as a `Discord_String` for the duration of a call.
///
/// # Safety contract for callers
///
/// The result borrows `s`. It must not outlive `s`, and must only be passed to
/// SDK functions that copy their input (which every setter does).
pub(crate) fn borrow(s: &str) -> sys::Discord_String {
    sys::Discord_String {
        // Cast away const: the SDK treats this as read-only despite the mutable type.
        ptr: s.as_ptr() as *mut u8,
        size: s.len(),
    }
}

/// Take ownership of a string returned by the SDK, freeing the SDK's buffer.
///
/// Invalid UTF-8 is replaced rather than rejected: the SDK sources these from
/// user-authored content, and a lossy name is far better than a panic deep
/// inside a callback.
///
/// # Safety
///
/// `s` must be a string the SDK just handed back and whose ownership was
/// transferred to us. Calling this twice on the same value double-frees.
pub(crate) unsafe fn take(s: sys::Discord_String) -> String {
    if s.ptr.is_null() || s.size == 0 {
        // A null pointer is never freeable; a zero-length buffer has nothing to read.
        if !s.ptr.is_null() {
            unsafe { sys::Discord_Free(s.ptr.cast()) };
        }
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(s.ptr, s.size) };
    let out = String::from_utf8_lossy(bytes).into_owned();
    unsafe { sys::Discord_Free(s.ptr.cast()) };
    out
}

/// Read a borrowed `Discord_String` without taking ownership.
///
/// Used for strings delivered as callback arguments, which the SDK frees itself
/// once the callback returns.
///
/// # Safety
///
/// `s` must point to a buffer valid for `s.size` bytes for the duration of the call.
pub(crate) unsafe fn view(s: sys::Discord_String) -> String {
    if s.ptr.is_null() || s.size == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(s.ptr, s.size) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Call an SDK getter of the shape `void Get(self, Discord_String* out)`.
///
/// # Safety
///
/// `f` must fully initialise the out-parameter and transfer ownership of it.
pub(crate) unsafe fn out<F>(f: F) -> String
where
    F: FnOnce(*mut sys::Discord_String),
{
    let mut raw = MaybeUninit::<sys::Discord_String>::uninit();
    f(raw.as_mut_ptr());
    unsafe { take(raw.assume_init()) }
}

/// Call an optional SDK getter of the shape `bool Get(self, Discord_String* out)`,
/// where `false` means the field is absent and the out-parameter is untouched.
///
/// # Safety
///
/// Same contract as [`out`], but `f` only initialises the out-parameter when it
/// returns `true`.
pub(crate) unsafe fn out_opt<F>(f: F) -> Option<String>
where
    F: FnOnce(*mut sys::Discord_String) -> bool,
{
    let mut raw = MaybeUninit::<sys::Discord_String>::uninit();
    if f(raw.as_mut_ptr()) {
        Some(unsafe { take(raw.assume_init()) })
    } else {
        None
    }
}

/// Run `f` with an optional borrowed string, matching the SDK's
/// "null pointer means clear this field" convention on setters.
pub(crate) fn with_opt<T, F>(value: Option<&str>, f: F) -> T
where
    F: FnOnce(*mut sys::Discord_String) -> T,
{
    match value {
        Some(v) => {
            let mut raw = borrow(v);
            f(&mut raw)
        }
        None => f(std::ptr::null_mut()),
    }
}
