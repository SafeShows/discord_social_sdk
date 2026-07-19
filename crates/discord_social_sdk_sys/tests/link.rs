//! Proves the SDK actually links and its entry points are callable at runtime.

use discord_social_sdk_sys::*;
use std::mem::MaybeUninit;

#[test]
fn client_roundtrips_through_the_real_library() {
    unsafe {
        let mut client = MaybeUninit::<Discord_Client>::uninit();
        Discord_Client_Init(client.as_mut_ptr());
        let mut client = client.assume_init();

        Discord_Client_SetApplicationId(&mut client, 1234567890);
        assert_eq!(Discord_Client_GetApplicationId(&mut client), 1234567890);

        // Pumping the callback queue with nothing pending must be a no-op, not a crash.
        Discord_RunCallbacks();

        Discord_Client_Drop(&mut client);
    }
}

#[test]
fn sdk_returns_owned_strings_we_can_read_and_free() {
    unsafe {
        let mut out = MaybeUninit::<Discord_String>::uninit();
        Discord_Client_GetDefaultPresenceScopes(out.as_mut_ptr());
        let s = out.assume_init();

        assert!(!s.ptr.is_null() && s.size > 0, "expected non-empty scopes");
        let text = std::str::from_utf8(std::slice::from_raw_parts(s.ptr, s.size))
            .expect("scopes should be valid UTF-8");
        assert!(text.contains("sdk"), "unexpected scopes: {text}");

        Discord_Free(s.ptr.cast());
    }
}
