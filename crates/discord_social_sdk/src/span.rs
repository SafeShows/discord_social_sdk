//! Converting `Discord_*Span` out-parameters into owned Rust [`Vec`]s.
//!
//! A span is a `{ ptr, size }` pair. When the SDK returns one, ownership of
//! *both* the array and every element in it transfers to the caller: each
//! element must be adopted individually, then the backing array released with
//! `Discord_Free`. Elements are moved out rather than cloned, so no extra
//! allocation happens per item.

use discord_social_sdk_sys as sys;
use std::mem::MaybeUninit;

/// Call a span-returning getter and adopt every element.
///
/// `wrap` takes each raw element by value and must produce a wrapper that owns
/// it — typically `|raw| unsafe { Type::from_raw(raw) }`.
///
/// # Safety
///
/// `f` must fully initialise the out-parameter and transfer ownership of the
/// array and its elements.
pub(crate) unsafe fn out<S, E, T, F, W>(f: F, wrap: W) -> Vec<T>
where
    S: SpanLayout<Elem = E>,
    F: FnOnce(*mut S),
    W: Fn(E) -> T,
{
    let mut raw = MaybeUninit::<S>::uninit();
    f(raw.as_mut_ptr());
    let span = unsafe { raw.assume_init() };

    let (ptr, size) = span.parts();
    if ptr.is_null() || size == 0 {
        if !ptr.is_null() {
            unsafe { sys::Discord_Free(ptr.cast()) };
        }
        return Vec::new();
    }

    let mut out = Vec::with_capacity(size);
    for i in 0..size {
        // Move the element out; the SDK will not touch it again.
        let elem = unsafe { std::ptr::read(ptr.add(i)) };
        out.push(wrap(elem));
    }
    // Free the array itself, not the elements — those are ours now.
    unsafe { sys::Discord_Free(ptr.cast()) };
    out
}

/// Structural access to a `Discord_*Span`.
///
/// Implemented via [`impl_span`] for each concrete span type, since bindgen
/// generates them as unrelated structs with identical shape.
pub(crate) trait SpanLayout {
    type Elem;
    fn parts(&self) -> (*mut Self::Elem, usize);
}

macro_rules! impl_span {
    ($($span:ty => $elem:ty),* $(,)?) => {
        $(
            impl $crate::span::SpanLayout for $span {
                type Elem = $elem;
                fn parts(&self) -> (*mut $elem, usize) {
                    (self.ptr, self.size)
                }
            }
        )*
    };
}

impl_span! {
    sys::Discord_ActivityButtonSpan => sys::Discord_ActivityButton,
    sys::Discord_UInt64Span => u64,
    sys::Discord_UserApplicationProfileHandleSpan => sys::Discord_UserApplicationProfileHandle,
    sys::Discord_LobbyMemberHandleSpan => sys::Discord_LobbyMemberHandle,
    sys::Discord_AudioDeviceSpan => sys::Discord_AudioDevice,
    sys::Discord_CallSpan => sys::Discord_Call,
    sys::Discord_UserMessageSummarySpan => sys::Discord_UserMessageSummary,
    sys::Discord_MessageHandleSpan => sys::Discord_MessageHandle,
    sys::Discord_RelationshipHandleSpan => sys::Discord_RelationshipHandle,
    sys::Discord_GuildMinimalSpan => sys::Discord_GuildMinimal,
    sys::Discord_GuildChannelSpan => sys::Discord_GuildChannel,
    sys::Discord_UserHandleSpan => sys::Discord_UserHandle,
}

/// Adopt a span delivered by value as a callback argument.
///
/// Callback spans are **owned**, exactly like the out-param case: the official
/// C++ wrapper adopts every element as `DiscordObjectState::Owned` and then
/// frees the backing array. Treating one as borrowed leaks every element plus
/// the array, so this moves the elements out and frees the array, same as
/// [`out`] — it differs only in how the span arrives.
///
/// # Safety
///
/// `span` must be a span the SDK just transferred to us, and must not be used
/// again afterwards.
pub(crate) unsafe fn take<S, E, T, W>(span: S, wrap: W) -> Vec<T>
where
    S: SpanLayout<Elem = E>,
    W: Fn(E) -> T,
{
    let (ptr, size) = span.parts();
    if ptr.is_null() || size == 0 {
        if !ptr.is_null() {
            unsafe { sys::Discord_Free(ptr.cast()) };
        }
        return Vec::new();
    }

    let mut out = Vec::with_capacity(size);
    for i in 0..size {
        // Move the element out; the SDK will not touch it again.
        let elem = unsafe { std::ptr::read(ptr.add(i)) };
        out.push(wrap(elem));
    }
    // Free the array itself, not the elements — those are ours now.
    unsafe { sys::Discord_Free(ptr.cast()) };
    out
}
