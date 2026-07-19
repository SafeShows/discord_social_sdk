//! Bridging Rust closures across the C callback boundary.
//!
//! Every callback-taking SDK function has the same shape:
//!
//! ```c
//! void Discord_Thing_DoAsync(Thing* self, Callback cb, Discord_FreeFn cbFree, void* cbData);
//! ```
//!
//! That triple is exactly enough to carry a boxed Rust closure plus the drop
//! glue to release it, so no global registry or handle table is needed.
//!
//! Two lifetimes exist:
//!
//! - **One-shot** — completion callbacks for async operations. The closure is an
//!   [`FnOnce`] stored as `Box<Option<F>>` and taken on first invocation.
//! - **Persistent** — event handlers installed with `Set*Callback`. The closure
//!   is [`FnMut`] and lives until replaced or until the owning handle drops.
//!
//! In both cases the SDK calls the `Discord_FreeFn` when it is done with the
//! userdata, which is where the box is reclaimed.
//!
//! # Arity
//!
//! Callback signatures vary too much for a single generic wrapper, so each call
//! site writes a small `extern "C"` trampoline and delegates the unsafe box
//! handling to [`dispatch_once`] / [`dispatch_mut`] here.

use discord_social_sdk_sys as sys;
use std::ffi::c_void;
use std::panic::AssertUnwindSafe;

/// Box a closure for a one-shot completion callback.
///
/// Returns the pointer to hand to the SDK as `userData`. Pair it with
/// [`free_fn`] parameterised over the same `Option<F>`.
pub(crate) fn once_userdata<F>(f: F) -> *mut c_void {
    Box::into_raw(Box::new(Some(f))).cast()
}

/// Box a closure for a persistent event callback.
pub(crate) fn persistent_userdata<F>(f: F) -> *mut c_void {
    Box::into_raw(Box::new(f)).cast()
}

/// The `Discord_FreeFn` that reclaims a userdata box of type `T`.
///
/// For one-shot callbacks `T` is `Option<F>`; for persistent ones it is `F`.
pub(crate) fn free_fn<T>() -> sys::Discord_FreeFn {
    unsafe extern "C" fn free_boxed<T>(ptr: *mut c_void) {
        if ptr.is_null() {
            return;
        }
        // Reconstitute and drop. Dropping user closures can panic, and unwinding
        // out of an `extern "C"` fn would abort the host process.
        let boxed = unsafe { Box::from_raw(ptr.cast::<T>()) };
        guard("callback userdata drop", move || drop(boxed));
    }
    Some(free_boxed::<T>)
}

/// Invoke a one-shot closure, consuming it.
///
/// Later invocations are ignored rather than treated as a fault: the SDK is not
/// expected to fire a completion twice, but silently no-oping is far better than
/// calling a moved-out closure.
///
/// # Safety
///
/// `userdata` must be the pointer produced by [`once_userdata::<F>`], still live.
pub(crate) unsafe fn dispatch_once<F>(userdata: *mut c_void, call: impl FnOnce(F)) {
    if userdata.is_null() {
        return;
    }
    let slot = unsafe { &mut *userdata.cast::<Option<F>>() };
    let Some(f) = slot.take() else { return };
    guard("callback", move || call(f));
}

/// Invoke a persistent closure by mutable reference.
///
/// # Safety
///
/// `userdata` must be the pointer produced by [`persistent_userdata::<F>`],
/// still live. The SDK delivers callbacks from the thread that calls
/// `run_callbacks`, so no two invocations overlap.
pub(crate) unsafe fn dispatch_mut<F>(userdata: *mut c_void, call: impl FnOnce(&mut F)) {
    if userdata.is_null() {
        return;
    }
    let f = unsafe { &mut *userdata.cast::<F>() };
    guard("callback", move || call(f));
}

/// Stop a panic from unwinding across the FFI boundary.
///
/// `extern "C"` functions abort on unwind, which would take down the whole host
/// application because one event handler panicked. Containing it here keeps the
/// SDK — and the game embedding it — running.
fn guard<R>(what: &str, f: impl FnOnce() -> R) -> Option<R> {
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => Some(v),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            eprintln!("discord_social_sdk: panic in {what}, ignored to protect FFI boundary: {msg}");
            None
        }
    }
}
