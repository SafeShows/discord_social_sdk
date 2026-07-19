//! OAuth2 authorization: arguments, PKCE code challenge/verifier pairs.
//!
//! Everything the SDK does on a user's behalf requires an OAuth2 access token.
//! This module holds the parameter objects for the flows that obtain one; the
//! functions that actually run those flows live on `Client`.
//!
//! # The flow the SDK models
//!
//! 1. **Authorize.** Build [`AuthorizationArgs`] — at minimum a scope string,
//!    which for most games is one of the client's default scope constants — and
//!    pass it to `Client::authorize`. The SDK opens Discord (or a browser),
//!    runs a loopback web server to catch the redirect, and hands back an
//!    *authorization code*. That code is not a token.
//! 2. **Exchange.** The code must be traded for an access token. Public clients
//!    can do this in-process with `Client::get_token`, which requires the PKCE
//!    pair described below. Apps with a backend should instead perform the
//!    exchange server-side, which is also what lets provisional accounts later
//!    "upgrade" into full Discord accounts.
//! 3. **Refresh.** Access tokens obtained this way expire (7 days for the
//!    device flow); the accompanying refresh token does not currently expire
//!    and is used to mint new access tokens.
//!
//! # PKCE
//!
//! When the token exchange happens inside the game process there is no client
//! secret to prove the exchange came from whoever started the flow. PKCE closes
//! that gap: `Client::create_authorization_code_verifier` generates an
//! [`AuthorizationCodeVerifier`], whose
//! [`challenge`](AuthorizationCodeVerifier::challenge) goes into
//! [`AuthorizationArgs::set_code_challenge`] and whose
//! [`verifier`](AuthorizationCodeVerifier::verifier) goes into the later token
//! exchange. This crate never generates these values itself — the SDK does.
//!
//! # Device flow
//!
//! On consoles, smart TVs and other limited-input devices,
//! [`DeviceAuthorizationArgs`] feeds `Client::get_token_from_device`, which
//! shows a short code the user enters on a second device instead of having them
//! type credentials in place.
//!
//! # Secrets
//!
//! Values here that must never reach logs, telemetry or crash reports:
//!
//! - the **code verifier** ([`AuthorizationCodeVerifier::verifier`]) — anyone
//!   holding it alongside an intercepted authorization code can redeem a token;
//! - the **state** and **nonce** ([`AuthorizationArgs::state`],
//!   [`AuthorizationArgs::nonce`]) — anti-CSRF and anti-replay values whose
//!   whole security property is being unpredictable.
//!
//! The [`Debug`] impls in this module redact all three. The *challenge* is a
//! hash of the verifier, travels in the clear as part of the authorization URL,
//! and is safe to print.

use discord_social_sdk_sys as sys;
use std::mem::MaybeUninit;

use crate::enums::{CodeChallengeMethod, IntegrationType};
use crate::handle::handle;
use crate::string;

handle! {
    /// The challenge half of a PKCE code verification pair.
    ///
    /// A hash of the secret verifier, sent to Discord when the authorization
    /// flow begins. It is not secret and may be logged.
    ///
    /// Normally obtained from [`AuthorizationCodeVerifier::challenge`] rather
    /// than built by hand; the SDK generates matching pairs.
    AuthorizationCodeChallenge(sys::Discord_AuthorizationCodeChallenge) {
        init: sys::Discord_AuthorizationCodeChallenge_Init,
        drop: sys::Discord_AuthorizationCodeChallenge_Drop,
        clone: sys::Discord_AuthorizationCodeChallenge_Clone,
    }
}

impl AuthorizationCodeChallenge {
    /// The method used to generate the challenge.
    ///
    /// The only method the SDK uses is SHA-256.
    pub fn method(&self) -> CodeChallengeMethod {
        // SAFETY: `raw_ptr` yields a valid initialised handle; this getter only reads.
        CodeChallengeMethod::from_raw(unsafe {
            sys::Discord_AuthorizationCodeChallenge_Method(self.raw_ptr())
        })
    }

    /// Set the method used to generate the challenge.
    pub fn set_method(&mut self, value: CodeChallengeMethod) {
        // SAFETY: `as_raw_mut` yields a valid initialised handle; the enum is a plain value.
        unsafe {
            sys::Discord_AuthorizationCodeChallenge_SetMethod(self.as_raw_mut(), value.into_raw())
        }
    }

    /// The challenge value.
    pub fn challenge(&self) -> String {
        // SAFETY: the getter fully initialises the out-parameter and transfers ownership of it.
        unsafe {
            string::out(|out| sys::Discord_AuthorizationCodeChallenge_Challenge(self.raw_ptr(), out))
        }
    }

    /// Set the challenge value.
    pub fn set_challenge(&mut self, value: &str) {
        // SAFETY: the SDK copies the string during the call, so borrowing `value` is sound.
        unsafe {
            sys::Discord_AuthorizationCodeChallenge_SetChallenge(
                self.as_raw_mut(),
                string::borrow(value),
            )
        }
    }
}

impl std::fmt::Debug for AuthorizationCodeChallenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationCodeChallenge")
            .field("method", &self.method())
            .field("challenge", &self.challenge())
            .finish()
    }
}

handle! {
    /// A PKCE code verification pair: the secret verifier and its challenge.
    ///
    /// Produced by `Client::create_authorization_code_verifier`. There is
    /// deliberately no constructor — the SDK generates the pair, and this crate
    /// implements no cryptography of its own.
    ///
    /// # Secrets
    ///
    /// [`verifier`](Self::verifier) is a secret. Do not log it, do not persist
    /// it beyond the lifetime of the flow, and do not send it anywhere other
    /// than the token exchange. The [`Debug`] impl redacts it.
    AuthorizationCodeVerifier(sys::Discord_AuthorizationCodeVerifier) {
        drop: sys::Discord_AuthorizationCodeVerifier_Drop,
        clone: sys::Discord_AuthorizationCodeVerifier_Clone,
    }
}

impl AuthorizationCodeVerifier {
    /// The challenge half of the pair.
    ///
    /// Pass this to [`AuthorizationArgs::set_code_challenge`] when starting the
    /// authorization flow.
    pub fn challenge(&self) -> AuthorizationCodeChallenge {
        let mut raw = MaybeUninit::<sys::Discord_AuthorizationCodeChallenge>::uninit();
        // SAFETY: the getter fully initialises the out-parameter and transfers ownership of the
        // handle to us, so wrapping it in an owning `AuthorizationCodeChallenge` is correct.
        unsafe {
            sys::Discord_AuthorizationCodeVerifier_Challenge(self.raw_ptr(), raw.as_mut_ptr());
            AuthorizationCodeChallenge::from_raw(raw.assume_init())
        }
    }

    /// Set the challenge half of the pair.
    ///
    /// The SDK copies the challenge, so `value` stays owned by the caller.
    pub fn set_challenge(&mut self, value: &AuthorizationCodeChallenge) {
        // SAFETY: both handles are valid and initialised. The SDK clones the challenge rather
        // than adopting the pointer — the official C++ wrapper passes a temporary here and lets
        // it destruct immediately after the call.
        unsafe {
            sys::Discord_AuthorizationCodeVerifier_SetChallenge(self.as_raw_mut(), value.raw_ptr())
        }
    }

    /// The verifier half of the pair.
    ///
    /// **This value is a secret.** It is presented during the token exchange to
    /// prove the exchange comes from whoever began the authorization flow. Never
    /// log it or share it.
    pub fn verifier(&self) -> String {
        // SAFETY: the getter fully initialises the out-parameter and transfers ownership of it.
        unsafe {
            string::out(|out| sys::Discord_AuthorizationCodeVerifier_Verifier(self.raw_ptr(), out))
        }
    }

    /// Set the verifier half of the pair.
    ///
    /// Only useful for restoring a pair the SDK generated earlier. Do not invent
    /// verifier values.
    pub fn set_verifier(&mut self, value: &str) {
        // SAFETY: the SDK copies the string during the call, so borrowing `value` is sound.
        unsafe {
            sys::Discord_AuthorizationCodeVerifier_SetVerifier(
                self.as_raw_mut(),
                string::borrow(value),
            )
        }
    }
}

impl std::fmt::Debug for AuthorizationCodeVerifier {
    /// Redacts the verifier; the challenge is printed in full.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationCodeVerifier")
            .field("challenge", &self.challenge())
            .field("verifier", &"<redacted>")
            .finish()
    }
}

handle! {
    /// Arguments to `Client::authorize`.
    ///
    /// Start from [`AuthorizationArgs::new`] and set at least
    /// [`set_scopes`](Self::set_scopes); everything else has a usable default.
    /// The `with_*` methods are the same setters in chainable form.
    ///
    /// # Secrets
    ///
    /// [`state`](Self::state) and [`nonce`](Self::nonce) are security values and
    /// are redacted by the [`Debug`] impl.
    AuthorizationArgs(sys::Discord_AuthorizationArgs) {
        init: sys::Discord_AuthorizationArgs_Init,
        drop: sys::Discord_AuthorizationArgs_Drop,
        clone: sys::Discord_AuthorizationArgs_Clone,
    }
}

impl AuthorizationArgs {
    /// The Discord application ID for the game.
    pub fn client_id(&self) -> u64 {
        // SAFETY: `raw_ptr` yields a valid initialised handle; this getter only reads.
        unsafe { sys::Discord_AuthorizationArgs_ClientId(self.raw_ptr()) }
    }

    /// Set the Discord application ID for the game.
    ///
    /// Optional; defaults to the value passed to `Client::set_application_id`.
    pub fn set_client_id(&mut self, value: u64) {
        // SAFETY: `as_raw_mut` yields a valid initialised handle.
        unsafe { sys::Discord_AuthorizationArgs_SetClientId(self.as_raw_mut(), value) }
    }

    /// The space-separated list of OAuth2 scopes being requested.
    pub fn scopes(&self) -> String {
        // SAFETY: the getter fully initialises the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_AuthorizationArgs_Scopes(self.raw_ptr(), out)) }
    }

    /// Set the space-separated list of OAuth2 scopes being requested.
    ///
    /// Most games should pass one of the client's default scope strings, which
    /// expand to `openid sdk.social_layer` or `openid sdk.social_layer_presence`
    /// plus everything those integrations need. Additional scopes are allowed,
    /// but each one is something the user must separately grant, so request only
    /// what is actually used.
    pub fn set_scopes(&mut self, value: &str) {
        // SAFETY: the SDK copies the string during the call, so borrowing `value` is sound.
        unsafe { sys::Discord_AuthorizationArgs_SetScopes(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The OAuth2 `state` value, if one was set explicitly.
    ///
    /// **Security-sensitive.** See [`set_state`](Self::set_state).
    pub fn state(&self) -> Option<String> {
        // SAFETY: the getter initialises and transfers the out-parameter only when it returns true.
        unsafe { string::out_opt(|out| sys::Discord_AuthorizationArgs_State(self.raw_ptr(), out)) }
    }

    /// Set the OAuth2 `state` value, or clear it with `None`.
    ///
    /// `state` guards against CSRF during authorization. Leaving it unset is
    /// recommended: the SDK then generates a secure random value and checks it
    /// itself. If overriding it for a custom flow, the value must be
    /// unpredictable, and it must not be logged.
    ///
    /// See the [OAuth2 state documentation] for details.
    ///
    /// [OAuth2 state documentation]: https://discord.com/developers/docs/topics/oauth2#state-and-security
    pub fn set_state(&mut self, value: Option<&str>) {
        string::with_opt(value, |ptr| {
            // SAFETY: `ptr` is either null (meaning "clear") or a valid `Discord_String` that
            // the SDK copies during the call.
            unsafe { sys::Discord_AuthorizationArgs_SetState(self.as_raw_mut(), ptr) }
        })
    }

    /// The OpenID Connect `nonce`, if one was set.
    ///
    /// **Security-sensitive.** See [`set_nonce`](Self::set_nonce).
    pub fn nonce(&self) -> Option<String> {
        // SAFETY: the getter initialises and transfers the out-parameter only when it returns true.
        unsafe { string::out_opt(|out| sys::Discord_AuthorizationArgs_Nonce(self.raw_ptr(), out)) }
    }

    /// Set the OpenID Connect `nonce`, or clear it with `None`.
    ///
    /// Generally only useful for backend integrations that consume ID tokens,
    /// where the nonce ties the returned token to this particular request. It
    /// must be unpredictable and must not be logged.
    pub fn set_nonce(&mut self, value: Option<&str>) {
        string::with_opt(value, |ptr| {
            // SAFETY: `ptr` is either null (meaning "clear") or a valid `Discord_String` that
            // the SDK copies during the call.
            unsafe { sys::Discord_AuthorizationArgs_SetNonce(self.as_raw_mut(), ptr) }
        })
    }

    /// The PKCE code challenge, if one was set.
    pub fn code_challenge(&self) -> Option<AuthorizationCodeChallenge> {
        let mut raw = MaybeUninit::<sys::Discord_AuthorizationCodeChallenge>::uninit();
        // SAFETY: the out-parameter is initialised — and its ownership transferred — only when
        // the call returns true, so `assume_init` is confined to that branch.
        unsafe {
            if sys::Discord_AuthorizationArgs_CodeChallenge(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(AuthorizationCodeChallenge::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Set the PKCE code challenge, or clear it with `None`.
    ///
    /// Required when the authorization code will be exchanged in-process with
    /// `Client::get_token`. Use `Client::create_authorization_code_verifier` to
    /// generate the pair, pass its
    /// [`challenge`](AuthorizationCodeVerifier::challenge) here, and keep the
    /// verifier for the exchange.
    ///
    /// The SDK copies the challenge, so `value` stays owned by the caller.
    pub fn set_code_challenge(&mut self, value: Option<&AuthorizationCodeChallenge>) {
        let ptr = value.map_or(std::ptr::null_mut(), |v| v.raw_ptr());
        // SAFETY: `ptr` is either null (meaning "clear") or a valid initialised challenge
        // handle, which the SDK clones rather than adopting.
        unsafe { sys::Discord_AuthorizationArgs_SetCodeChallenge(self.as_raw_mut(), ptr) }
    }

    /// The installation context the app will be authorized in, if set.
    pub fn integration_type(&self) -> Option<IntegrationType> {
        let mut raw = MaybeUninit::<sys::Discord_IntegrationType>::uninit();
        // SAFETY: the out-parameter is initialised only when the call returns true, so
        // `assume_init` is confined to that branch.
        unsafe {
            if sys::Discord_AuthorizationArgs_IntegrationType(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(IntegrationType::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Set the installation context, or clear it with `None`.
    ///
    /// See the [installation context documentation].
    ///
    /// [installation context documentation]: https://discord.com/developers/docs/resources/application#installation-context
    pub fn set_integration_type(&mut self, value: Option<IntegrationType>) {
        let mut raw = value.map(IntegrationType::into_raw);
        let ptr = raw
            .as_mut()
            .map_or(std::ptr::null_mut(), |v| v as *mut sys::Discord_IntegrationType);
        // SAFETY: `ptr` is either null (meaning "clear") or points at `raw`, which outlives the
        // call; the SDK reads the value rather than retaining the pointer.
        unsafe { sys::Discord_AuthorizationArgs_SetIntegrationType(self.as_raw_mut(), ptr) }
    }

    /// The custom URI scheme used for mobile redirects, if set.
    pub fn custom_scheme_param(&self) -> Option<String> {
        // SAFETY: the getter initialises and transfers the out-parameter only when it returns true.
        unsafe {
            string::out_opt(|out| {
                sys::Discord_AuthorizationArgs_CustomSchemeParam(self.raw_ptr(), out)
            })
        }
    }

    /// Set the custom URI scheme used for mobile redirects, or clear it with `None`.
    ///
    /// Setting this to `mygame` produces redirect URIs of the form
    /// `mygame:/authorize/callback`. When unset, the standard Discord form
    /// `discord-123456789:/authorize/callback` is used. Useful for
    /// distinguishing several games from one developer, or for avoiding
    /// conflicts with other apps.
    pub fn set_custom_scheme_param(&mut self, value: Option<&str>) {
        string::with_opt(value, |ptr| {
            // SAFETY: `ptr` is either null (meaning "clear") or a valid `Discord_String` that
            // the SDK copies during the call.
            unsafe { sys::Discord_AuthorizationArgs_SetCustomSchemeParam(self.as_raw_mut(), ptr) }
        })
    }

    /// Chainable [`set_client_id`](Self::set_client_id).
    #[must_use]
    pub fn with_client_id(mut self, value: u64) -> Self {
        self.set_client_id(value);
        self
    }

    /// Chainable [`set_scopes`](Self::set_scopes).
    #[must_use]
    pub fn with_scopes(mut self, value: &str) -> Self {
        self.set_scopes(value);
        self
    }

    /// Chainable [`set_state`](Self::set_state).
    #[must_use]
    pub fn with_state(mut self, value: Option<&str>) -> Self {
        self.set_state(value);
        self
    }

    /// Chainable [`set_nonce`](Self::set_nonce).
    #[must_use]
    pub fn with_nonce(mut self, value: Option<&str>) -> Self {
        self.set_nonce(value);
        self
    }

    /// Chainable [`set_code_challenge`](Self::set_code_challenge).
    #[must_use]
    pub fn with_code_challenge(mut self, value: Option<&AuthorizationCodeChallenge>) -> Self {
        self.set_code_challenge(value);
        self
    }

    /// Chainable [`set_integration_type`](Self::set_integration_type).
    #[must_use]
    pub fn with_integration_type(mut self, value: Option<IntegrationType>) -> Self {
        self.set_integration_type(value);
        self
    }

    /// Chainable [`set_custom_scheme_param`](Self::set_custom_scheme_param).
    #[must_use]
    pub fn with_custom_scheme_param(mut self, value: Option<&str>) -> Self {
        self.set_custom_scheme_param(value);
        self
    }
}

impl std::fmt::Debug for AuthorizationArgs {
    /// Redacts `state` and `nonce`, reporting only whether each is present.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn redacted(value: &Option<String>) -> &'static str {
            match value {
                Some(_) => "<redacted>",
                None => "None",
            }
        }

        f.debug_struct("AuthorizationArgs")
            .field("client_id", &self.client_id())
            .field("scopes", &self.scopes())
            .field("state", &redacted(&self.state()))
            .field("nonce", &redacted(&self.nonce()))
            .field("code_challenge", &self.code_challenge())
            .field("integration_type", &self.integration_type())
            .field("custom_scheme_param", &self.custom_scheme_param())
            .finish()
    }
}

handle! {
    /// Arguments to `Client::get_token_from_device`.
    ///
    /// The device authorization flow serves limited-input devices — consoles,
    /// smart TVs — where the user completes login on a second device.
    DeviceAuthorizationArgs(sys::Discord_DeviceAuthorizationArgs) {
        init: sys::Discord_DeviceAuthorizationArgs_Init,
        drop: sys::Discord_DeviceAuthorizationArgs_Drop,
        clone: sys::Discord_DeviceAuthorizationArgs_Clone,
    }
}

impl DeviceAuthorizationArgs {
    /// The Discord application ID for the game.
    pub fn client_id(&self) -> u64 {
        // SAFETY: `raw_ptr` yields a valid initialised handle; this getter only reads.
        unsafe { sys::Discord_DeviceAuthorizationArgs_ClientId(self.raw_ptr()) }
    }

    /// Set the Discord application ID for the game.
    ///
    /// Optional; defaults to the value passed to `Client::set_application_id`.
    pub fn set_client_id(&mut self, value: u64) {
        // SAFETY: `as_raw_mut` yields a valid initialised handle.
        unsafe { sys::Discord_DeviceAuthorizationArgs_SetClientId(self.as_raw_mut(), value) }
    }

    /// The space-separated list of OAuth2 scopes being requested.
    pub fn scopes(&self) -> String {
        // SAFETY: the getter fully initialises the out-parameter and transfers ownership of it.
        unsafe {
            string::out(|out| sys::Discord_DeviceAuthorizationArgs_Scopes(self.raw_ptr(), out))
        }
    }

    /// Set the space-separated list of OAuth2 scopes being requested.
    ///
    /// The same guidance as [`AuthorizationArgs::set_scopes`] applies: prefer
    /// the client's default scope strings and add extras only when needed.
    pub fn set_scopes(&mut self, value: &str) {
        // SAFETY: the SDK copies the string during the call, so borrowing `value` is sound.
        unsafe {
            sys::Discord_DeviceAuthorizationArgs_SetScopes(self.as_raw_mut(), string::borrow(value))
        }
    }

    /// Chainable [`set_client_id`](Self::set_client_id).
    #[must_use]
    pub fn with_client_id(mut self, value: u64) -> Self {
        self.set_client_id(value);
        self
    }

    /// Chainable [`set_scopes`](Self::set_scopes).
    #[must_use]
    pub fn with_scopes(mut self, value: &str) -> Self {
        self.set_scopes(value);
        self
    }
}

impl std::fmt::Debug for DeviceAuthorizationArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceAuthorizationArgs")
            .field("client_id", &self.client_id())
            .field("scopes", &self.scopes())
            .finish()
    }
}
