//! RAII machinery shared by every SDK handle type.
//!
//! Every handle in `cdiscord.h` is `struct { void* opaque; }` paired with
//! `_Init`, `_Drop` and usually `_Clone`. That regularity means one macro can
//! generate the ownership half of all ~40 wrapper types, leaving each module to
//! write only its accessors.
//!
//! # Everything is owned
//!
//! It would be reasonable to expect callbacks to *lend* handles, and for the
//! wrapper to need a borrowed view type. They do not. Checking the official C++
//! wrapper shows it adopts every handle, string, span and `ClientResult` a
//! callback receives as `DiscordObjectState::Owned` and lets the destructor run.
//! So every handle that reaches Rust is owned and must be dropped exactly once,
//! and no borrowed-handle type is needed.

/// Generate the ownership half of a handle wrapper.
///
/// Produces the newtype, `Drop`, `from_raw`/`as_raw`/`as_raw_mut`/`raw_ptr`,
/// and — when a `clone:` function is supplied — `Clone`. Accessors are written
/// by hand in each type's own module.
///
/// The `init:` arm is omitted for handles the SDK only ever hands back (such as
/// `MessageHandle`), which have no meaningful default state.
macro_rules! handle {
    (
        $(#[$meta:meta])*
        $name:ident($raw:path) {
            $(init: $init:path,)?
            drop: $drop:path,
            $(clone: $clone:path,)?
        }
    ) => {
        $(#[$meta])*
        pub struct $name {
            pub(crate) raw: $raw,
        }

        impl $name {
            /// Take ownership of a raw handle.
            ///
            /// # Safety
            ///
            /// `raw` must be an initialised handle that nothing else will drop.
            #[allow(dead_code)]
            pub(crate) unsafe fn from_raw(raw: $raw) -> Self {
                Self { raw }
            }

            /// Borrow the underlying raw handle.
            #[allow(dead_code)]
            pub(crate) fn as_raw(&self) -> *const $raw {
                &self.raw
            }

            /// Mutably borrow the underlying raw handle.
            #[allow(dead_code)]
            pub(crate) fn as_raw_mut(&mut self) -> *mut $raw {
                &mut self.raw
            }

            /// A mutable raw pointer obtained from a shared reference.
            ///
            /// Almost every SDK *getter* takes `self` as non-const even though it
            /// only reads, which would otherwise force `&mut self` on the whole
            /// read API. The official C++ wrapper casts away const in exactly the
            /// same places, so this is sound for read-only calls.
            ///
            /// Use this only for functions that genuinely read. Anything that
            /// mutates must go through [`as_raw_mut`](Self::as_raw_mut) and a
            /// real `&mut self`.
            #[allow(dead_code)]
            pub(crate) fn raw_ptr(&self) -> *mut $raw {
                &self.raw as *const $raw as *mut $raw
            }

            $(
                /// Create a new, empty handle.
                pub fn new() -> Self {
                    let mut raw = ::std::mem::MaybeUninit::<$raw>::uninit();
                    unsafe {
                        $init(raw.as_mut_ptr());
                        Self { raw: raw.assume_init() }
                    }
                }
            )?
        }

        $(
            impl Default for $name {
                fn default() -> Self {
                    // Referenced so the macro arm binds; `new` is what it generates.
                    let _ = $init;
                    Self::new()
                }
            }
        )?

        impl Drop for $name {
            fn drop(&mut self) {
                unsafe { $drop(&mut self.raw) }
            }
        }

        $(
            impl Clone for $name {
                fn clone(&self) -> Self {
                    let mut raw = ::std::mem::MaybeUninit::<$raw>::uninit();
                    unsafe {
                        // Argument order is (destination, source).
                        $clone(raw.as_mut_ptr(), &self.raw);
                        Self { raw: raw.assume_init() }
                    }
                }
            }
        )?
    };
}

pub(crate) use handle;
