//! Authorization, token exchange, and provisional accounts.
//!
//! # The standard flow
//!
//! 1. Create a PKCE verifier with [`Client::create_authorization_code_verifier`].
//! 2. Put its challenge on an [`AuthorizationArgs`] and call [`Client::authorize`],
//!    which opens Discord's consent screen and yields an authorization code.
//! 3. Exchange that code for a token with [`Client::token`], passing the
//!    verifier's secret.
//! 4. Hand the access token to [`Client::update_token`], then
//!    [`connect`](Client::connect).
//!
//! # Secrets
//!
//! Access tokens, refresh tokens, and the PKCE verifier are credentials. [`Token`]
//! redacts them in its [`Debug`] output, but anything read out of it via the
//! accessors is plaintext — do not log those values.

use super::Client;
use crate::auth::{AuthorizationArgs, AuthorizationCodeVerifier, DeviceAuthorizationArgs};
use crate::enums::{AuthorizationTokenType, ExternalAuthType};
use crate::error::{Result, to_result};
use crate::{callback, string};
use discord_social_sdk_sys as sys;
use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::time::Duration;

/// The credentials returned by a token exchange.
///
/// Every string field is sensitive. [`Debug`] redacts them.
#[derive(Clone, PartialEq, Eq)]
pub struct Token {
    access_token: String,
    refresh_token: String,
    token_type: AuthorizationTokenType,
    expires_in: i32,
    scopes: String,
}

impl Token {
    /// The access token to authenticate with. **Secret.**
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// The refresh token used to obtain a new access token. **Secret.**
    ///
    /// Empty for exchanges that do not issue one.
    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }

    /// The kind of token issued.
    pub fn token_type(&self) -> AuthorizationTokenType {
        self.token_type
    }

    /// How long the access token remains valid.
    pub fn expires_in(&self) -> Duration {
        Duration::from_secs(self.expires_in.max(0) as u64)
    }

    /// The space-separated OAuth2 scopes the token grants.
    pub fn scopes(&self) -> &str {
        &self.scopes
    }
}

impl std::fmt::Debug for Token {
    /// Redacts both tokens; they are credentials and must not reach logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in())
            .field("scopes", &self.scopes)
            .finish()
    }
}

/// The authorization code and redirect URI produced by [`Client::authorize`].
///
/// The code is single-use and short-lived, and is exchanged for a [`Token`].
#[derive(Clone, PartialEq, Eq)]
pub struct AuthorizationCode {
    code: String,
    redirect_uri: String,
}

impl AuthorizationCode {
    /// The authorization code. **Secret** — exchange it, do not log it.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The redirect URI the code was issued against.
    ///
    /// Must be passed unchanged to [`Client::token`].
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }
}

impl std::fmt::Debug for AuthorizationCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationCode")
            .field("code", &"<redacted>")
            .field("redirect_uri", &self.redirect_uri)
            .finish()
    }
}

/// The identity returned by [`Client::fetch_current_user`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentUserInfo {
    /// The user's Discord snowflake id.
    pub id: u64,
    /// The user's name.
    pub name: String,
}

/// Trampoline shared by every `Discord_Client_TokenExchangeCallback` site.
unsafe extern "C" fn token_tramp<F>(
    result: *mut sys::Discord_ClientResult,
    access_token: sys::Discord_String,
    refresh_token: sys::Discord_String,
    token_type: sys::Discord_AuthorizationTokenType,
    expires_in: i32,
    scopes: sys::Discord_String,
    userdata: *mut c_void,
) where
    F: FnOnce(Result<Token>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F`. The strings are transferred to us —
    // the C++ wrapper frees them after the delegate runs — so they are taken.
    //
    // The token is built BEFORE the result is inspected, and deliberately not
    // inside `Result::map`: the SDK hands over these strings whether or not the
    // request succeeded, so claiming them only on success would leak all three
    // on every failed exchange. Once taken they are ordinary `String`s, and
    // dropping the unused `Token` on the error path frees them.
    unsafe {
        callback::dispatch_once::<F>(userdata, |f| {
            let token = Token {
                access_token: string::take(access_token),
                refresh_token: string::take(refresh_token),
                token_type: AuthorizationTokenType::from_raw(token_type),
                expires_in,
                scopes: string::take(scopes),
            };
            f(to_result(result).map(|()| token))
        })
    }
}

/// Trampoline shared by every callback that reports only success or failure.
unsafe extern "C" fn result_tramp<F>(result: *mut sys::Discord_ClientResult, userdata: *mut c_void)
where
    F: FnOnce(Result<()>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result))) }
}

impl Client {
    /// Whether the client currently holds a valid token.
    pub fn is_authenticated(&self) -> bool {
        // SAFETY: a plain by-value read of an initialised handle.
        unsafe { sys::Discord_Client_IsAuthenticated(self.raw_ptr()) }
    }

    /// Generate a fresh PKCE verifier and its matching challenge.
    ///
    /// The verifier is a secret; keep it until the code exchange and do not log it.
    pub fn create_authorization_code_verifier(&self) -> AuthorizationCodeVerifier {
        let mut raw = MaybeUninit::<sys::Discord_AuthorizationCodeVerifier>::uninit();
        // SAFETY: the call fully initialises the handle and transfers ownership.
        unsafe {
            sys::Discord_Client_CreateAuthorizationCodeVerifier(self.raw_ptr(), raw.as_mut_ptr());
            AuthorizationCodeVerifier::from_raw(raw.assume_init())
        }
    }

    /// Begin the OAuth2 authorization flow.
    ///
    /// Opens Discord's consent screen. On success the callback receives an
    /// [`AuthorizationCode`] to exchange via [`token`](Self::token).
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn authorize<F>(&mut self, args: &mut AuthorizationArgs, callback: F)
    where
        F: FnOnce(Result<AuthorizationCode>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            code: sys::Discord_String,
            redirect_uri: sys::Discord_String,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<AuthorizationCode>) + 'static,
        {
            // SAFETY: the strings are transferred to us and must be freed, so they are taken.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    // Claimed before the result is inspected: the SDK transfers
                    // both strings regardless of outcome, so taking them only on
                    // success would leak them on every failed authorization.
                    let code = AuthorizationCode {
                        code: string::take(code),
                        redirect_uri: string::take(redirect_uri),
                    };
                    f(to_result(result).map(|()| code))
                })
            }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_Authorize(
                self.as_raw_mut(),
                args.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Cancel an in-flight [`authorize`](Self::authorize).
    pub fn abort_authorize(&mut self) {
        // SAFETY: aborting only requires an initialised handle.
        unsafe { sys::Discord_Client_AbortAuthorize(self.as_raw_mut()) }
    }

    /// Exchange an authorization code for a [`Token`].
    ///
    /// `code_verifier` is the secret from the [`AuthorizationCodeVerifier`] whose
    /// challenge was used in [`authorize`](Self::authorize).
    pub fn token<F>(
        &mut self,
        application_id: u64,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
        callback: F,
    ) where
        F: FnOnce(Result<Token>) + 'static,
    {
        // SAFETY: all strings are copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_GetToken(
                self.as_raw_mut(),
                application_id,
                string::borrow(code),
                string::borrow(code_verifier),
                string::borrow(redirect_uri),
                Some(token_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Exchange a refresh token for a new [`Token`].
    pub fn refresh_token<F>(&mut self, application_id: u64, refresh_token: &str, callback: F)
    where
        F: FnOnce(Result<Token>) + 'static,
    {
        // SAFETY: the token string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_RefreshToken(
                self.as_raw_mut(),
                application_id,
                string::borrow(refresh_token),
                Some(token_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Install a token on the client, making it usable for authenticated calls.
    ///
    /// Call before [`connect`](Self::connect).
    pub fn update_token<F>(&mut self, token_type: AuthorizationTokenType, token: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the token string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_UpdateToken(
                self.as_raw_mut(),
                token_type.into_raw(),
                string::borrow(token),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Revoke a token, invalidating it server-side.
    pub fn revoke_token<F>(&mut self, application_id: u64, token: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the token string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_RevokeToken(
                self.as_raw_mut(),
                application_id,
                string::borrow(token),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Look up the user a token belongs to, without connecting.
    pub fn fetch_current_user<F>(
        &mut self,
        token_type: AuthorizationTokenType,
        token: &str,
        callback: F,
    ) where
        F: FnOnce(Result<CurrentUserInfo>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            id: u64,
            name: sys::Discord_String,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<CurrentUserInfo>) + 'static,
        {
            // SAFETY: `name` is transferred to us and must be freed, so it is taken.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    // Claimed unconditionally; the SDK transfers `name` even when
                    // the lookup failed.
                    let info = CurrentUserInfo {
                        id,
                        name: string::take(name),
                    };
                    f(to_result(result).map(|()| info))
                })
            }
        }
        // SAFETY: the token string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_FetchCurrentUser(
                self.as_raw_mut(),
                token_type.into_raw(),
                string::borrow(token),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Exchange a parent application's token for a child application's token.
    ///
    /// Used when several applications share one Discord identity.
    pub fn exchange_child_token<F>(
        &mut self,
        parent_application_token: &str,
        child_application_id: u64,
        callback: F,
    ) where
        F: FnOnce(Result<Token>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            access_token: sys::Discord_String,
            token_type: sys::Discord_AuthorizationTokenType,
            expires_in: i32,
            scopes: sys::Discord_String,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<Token>) + 'static,
        {
            // SAFETY: the strings are transferred to us and must be freed, so they are taken. This
            // exchange issues no refresh token, so that field is left empty.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    // Claimed unconditionally; both strings are transferred even
                    // when the exchange failed.
                    let token = Token {
                        access_token: string::take(access_token),
                        refresh_token: String::new(),
                        token_type: AuthorizationTokenType::from_raw(token_type),
                        expires_in,
                        scopes: string::take(scopes),
                    };
                    f(to_result(result).map(|()| token))
                })
            }
        }
        // SAFETY: the token string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_ExchangeChildToken(
                self.as_raw_mut(),
                string::borrow(parent_application_token),
                child_application_id,
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Be notified when the current token is about to expire.
    ///
    /// Refresh it from the handler to keep the session alive.
    pub fn on_token_expiration<F>(&mut self, callback: F)
    where
        F: FnMut() + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut() + 'static>(userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f()) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetTokenExpirationCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    // ---- Device authorization (consoles and other input-constrained devices) ----

    /// Obtain a token through the device authorization flow.
    ///
    /// For devices where a browser consent screen is impractical. The user is
    /// shown a code to enter on another device.
    pub fn token_from_device<F>(&mut self, args: &mut DeviceAuthorizationArgs, callback: F)
    where
        F: FnOnce(Result<Token>) + 'static,
    {
        // SAFETY: `args` is read during the call and not retained.
        unsafe {
            sys::Discord_Client_GetTokenFromDevice(
                self.as_raw_mut(),
                args.as_raw_mut(),
                Some(token_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Cancel an in-flight [`token_from_device`](Self::token_from_device).
    pub fn abort_token_from_device(&mut self) {
        // SAFETY: aborting only requires an initialised handle.
        unsafe { sys::Discord_Client_AbortGetTokenFromDevice(self.as_raw_mut()) }
    }

    /// Show Discord's device authorization screen for `user_code`.
    pub fn open_authorize_device_screen(&mut self, client_id: u64, user_code: &str) {
        // SAFETY: the code string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_OpenAuthorizeDeviceScreen(
                self.as_raw_mut(),
                client_id,
                string::borrow(user_code),
            )
        }
    }

    /// Close the device authorization screen.
    pub fn close_authorize_device_screen(&mut self) {
        // SAFETY: only requires an initialised handle.
        unsafe { sys::Discord_Client_CloseAuthorizeDeviceScreen(self.as_raw_mut()) }
    }

    /// Be notified when the device authorization screen is dismissed.
    pub fn on_authorize_device_screen_closed<F>(&mut self, callback: F)
    where
        F: FnMut() + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut() + 'static>(userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f()) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetAuthorizeDeviceScreenClosedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Observe incoming authorization requests from the Discord client.
    ///
    /// Paired with [`remove_authorize_request_callback`](Self::remove_authorize_request_callback).
    pub fn register_authorize_request_callback<F>(&mut self, callback: F)
    where
        F: FnMut() + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut() + 'static>(userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f()) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_RegisterAuthorizeRequestCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Stop observing authorization requests.
    pub fn remove_authorize_request_callback(&mut self) {
        // SAFETY: only requires an initialised handle.
        unsafe { sys::Discord_Client_RemoveAuthorizeRequestCallback(self.as_raw_mut()) }
    }

    // ---- Provisional accounts ----
    //
    // A provisional account lets a player use the SDK before linking a real
    // Discord account; it can later be merged into one.

    /// Obtain a token for a provisional account backed by an external identity.
    pub fn provisional_token<F>(
        &mut self,
        application_id: u64,
        external_auth_type: ExternalAuthType,
        external_auth_token: &str,
        callback: F,
    ) where
        F: FnOnce(Result<Token>) + 'static,
    {
        // SAFETY: the token string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_GetProvisionalToken(
                self.as_raw_mut(),
                application_id,
                external_auth_type.into_raw(),
                string::borrow(external_auth_token),
                Some(token_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Exchange an authorization code while merging a provisional account.
    #[allow(clippy::too_many_arguments)] // Mirrors the C function exactly.
    pub fn token_from_provisional_merge<F>(
        &mut self,
        application_id: u64,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
        external_auth_type: ExternalAuthType,
        external_auth_token: &str,
        callback: F,
    ) where
        F: FnOnce(Result<Token>) + 'static,
    {
        // SAFETY: all strings are copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_GetTokenFromProvisionalMerge(
                self.as_raw_mut(),
                application_id,
                string::borrow(code),
                string::borrow(code_verifier),
                string::borrow(redirect_uri),
                external_auth_type.into_raw(),
                string::borrow(external_auth_token),
                Some(token_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Device-authorization variant of [`token_from_provisional_merge`](Self::token_from_provisional_merge).
    pub fn token_from_device_provisional_merge<F>(
        &mut self,
        args: &mut DeviceAuthorizationArgs,
        external_auth_type: ExternalAuthType,
        external_auth_token: &str,
        callback: F,
    ) where
        F: FnOnce(Result<Token>) + 'static,
    {
        // SAFETY: `args` is read during the call; the token string is copied.
        unsafe {
            sys::Discord_Client_GetTokenFromDeviceProvisionalMerge(
                self.as_raw_mut(),
                args.as_raw_mut(),
                external_auth_type.into_raw(),
                string::borrow(external_auth_token),
                Some(token_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Split an external identity back out into its own provisional account.
    pub fn unmerge_into_provisional_account<F>(
        &mut self,
        application_id: u64,
        external_auth_type: ExternalAuthType,
        external_auth_token: &str,
        callback: F,
    ) where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the token string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_UnmergeIntoProvisionalAccount(
                self.as_raw_mut(),
                application_id,
                external_auth_type.into_raw(),
                string::borrow(external_auth_token),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Rename a provisional account.
    pub fn update_provisional_account_display_name<F>(&mut self, name: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the name string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_UpdateProvisionalAccountDisplayName(
                self.as_raw_mut(),
                string::borrow(name),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Tell the SDK a provisional account merge finished, and whether it worked.
    pub fn provisional_user_merge_completed(&mut self, success: bool) {
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe { sys::Discord_Client_ProvisionalUserMergeCompleted(self.as_raw_mut(), success) }
    }
}
