//! The [`Client`] — the entry point to the SDK.
//!
//! A `Client` owns the connection to Discord and is the receiver for nearly
//! every operation the SDK offers. Its surface is large, so it is split across
//! several files that each add an `impl Client` block:
//!
//! - this module — construction, connection lifecycle, status, logging, version
//! - `auth` — OAuth2, tokens, provisional accounts
//! - `voice` — calls, audio devices, capture and playback settings
//! - `lobby` — creating, joining and observing lobbies
//! - `messaging` — sending and reading messages
//! - `relationships` — friends, blocking, friend requests
//! - `presence` — rich presence and activity invites
//!
//! # Lifecycle
//!
//! A client must be given an application id, connected, and then driven by
//! [`run_callbacks`](crate::run_callbacks). It is not usable until its status
//! reaches [`ClientStatus::Ready`].
//!
//! ```no_run
//! use discord_social_sdk::{Client, enums::ClientStatus, run_callbacks};
//!
//! let mut client = Client::new();
//! client.set_application_id(1234567890);
//! client.on_status_changed(|status, error, detail| {
//!     if let Some(error) = error {
//!         eprintln!("connection error: {error:?} ({detail})");
//!     }
//!     if status == ClientStatus::Ready {
//!         println!("ready");
//!     }
//! });
//! client.connect();
//!
//! loop {
//!     run_callbacks();
//!     std::thread::sleep(std::time::Duration::from_millis(16));
//! }
//! ```

mod auth;
mod lobby;
mod messaging;
mod presence;
mod relationships;
mod voice;

pub use auth::{AuthorizationCode, CurrentUserInfo, Token};
pub use voice::AudioFrame;

use crate::enums::{AudioSystem, ClientError, ClientStatus, ClientThread, LoggingSeverity};
use crate::handle::handle;
use crate::user::User;
use crate::{callback, string};
use discord_social_sdk_sys as sys;
use std::mem::MaybeUninit;

handle! {
    /// Configuration for a [`Client`], for the cases [`Client::new`] does not cover.
    ///
    /// Only needed when pointing the SDK at non-production endpoints or tuning
    /// its threading and audio behaviour. Most applications should use
    /// [`Client::new`].
    ClientCreateOptions(sys::Discord_ClientCreateOptions) {
        init: sys::Discord_ClientCreateOptions_Init,
        drop: sys::Discord_ClientCreateOptions_Drop,
        clone: sys::Discord_ClientCreateOptions_Clone,
    }
}

impl ClientCreateOptions {
    /// The base URL for Discord's web endpoints.
    pub fn web_base(&self) -> String {
        // SAFETY: the getter fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_ClientCreateOptions_WebBase(self.raw_ptr(), out)) }
    }

    /// Override the base URL for Discord's web endpoints.
    pub fn set_web_base(&mut self, value: &str) {
        // SAFETY: the SDK copies the string during the call.
        unsafe {
            sys::Discord_ClientCreateOptions_SetWebBase(self.as_raw_mut(), string::borrow(value))
        }
    }

    /// The base URL for Discord's API endpoints.
    pub fn api_base(&self) -> String {
        // SAFETY: the getter fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_ClientCreateOptions_ApiBase(self.raw_ptr(), out)) }
    }

    /// Override the base URL for Discord's API endpoints.
    pub fn set_api_base(&mut self, value: &str) {
        // SAFETY: the SDK copies the string during the call.
        unsafe {
            sys::Discord_ClientCreateOptions_SetApiBase(self.as_raw_mut(), string::borrow(value))
        }
    }

    /// Which audio backend the SDK should use.
    ///
    /// Experimental; the default is appropriate for nearly all applications.
    pub fn experimental_audio_system(&self) -> AudioSystem {
        // SAFETY: a plain by-value read of an initialised handle.
        AudioSystem::from_raw(unsafe {
            sys::Discord_ClientCreateOptions_ExperimentalAudioSystem(self.raw_ptr())
        })
    }

    /// Select the audio backend. Experimental.
    pub fn set_experimental_audio_system(&mut self, value: AudioSystem) {
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe {
            sys::Discord_ClientCreateOptions_SetExperimentalAudioSystem(
                self.as_raw_mut(),
                value.into_raw(),
            )
        }
    }

    /// Whether Android communication mode is suppressed for Bluetooth devices.
    ///
    /// Experimental, and only meaningful on Android.
    pub fn experimental_android_prevent_comms_for_bluetooth(&self) -> bool {
        // SAFETY: a plain by-value read of an initialised handle.
        unsafe {
            sys::Discord_ClientCreateOptions_ExperimentalAndroidPreventCommsForBluetooth(
                self.raw_ptr(),
            )
        }
    }

    /// Suppress Android communication mode for Bluetooth devices. Experimental.
    pub fn set_experimental_android_prevent_comms_for_bluetooth(&mut self, value: bool) {
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe {
            sys::Discord_ClientCreateOptions_SetExperimentalAndroidPreventCommsForBluetooth(
                self.as_raw_mut(),
                value,
            )
        }
    }

    /// The CPU affinity mask applied to the SDK's threads, if one is set.
    pub fn cpu_affinity_mask(&self) -> Option<u64> {
        let mut out = MaybeUninit::<u64>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns true.
        unsafe {
            sys::Discord_ClientCreateOptions_CpuAffinityMask(self.raw_ptr(), out.as_mut_ptr())
                .then(|| out.assume_init())
        }
    }

    /// Pin the SDK's threads to a set of CPUs, or `None` to leave them unpinned.
    pub fn set_cpu_affinity_mask(&mut self, value: Option<u64>) {
        // SAFETY: a null pointer is the SDK's documented "unset" signal.
        unsafe {
            match value {
                Some(mut mask) => sys::Discord_ClientCreateOptions_SetCpuAffinityMask(
                    self.as_raw_mut(),
                    &mut mask,
                ),
                None => sys::Discord_ClientCreateOptions_SetCpuAffinityMask(
                    self.as_raw_mut(),
                    std::ptr::null_mut(),
                ),
            }
        }
    }
}

impl std::fmt::Debug for ClientCreateOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientCreateOptions")
            .field("api_base", &self.api_base())
            .field("web_base", &self.web_base())
            .field(
                "experimental_audio_system",
                &self.experimental_audio_system(),
            )
            .field("cpu_affinity_mask", &self.cpu_affinity_mask())
            .finish()
    }
}

handle! {
    /// The connection to Discord and the entry point to every SDK operation.
    ///
    /// See the [module documentation](self) for the lifecycle and for where each
    /// group of methods is defined.
    ///
    /// A `Client` is deliberately not [`Send`]: the SDK expects to be driven from
    /// a single thread, and every callback is delivered on whichever thread calls
    /// [`run_callbacks`](crate::run_callbacks).
    Client(sys::Discord_Client) {
        init: sys::Discord_Client_Init,
        drop: sys::Discord_Client_Drop,
    }
}

impl Client {
    /// Create a client against explicit API and web base URLs.
    ///
    /// Only needed for non-production endpoints; prefer [`Client::new`].
    pub fn with_bases(api_base: &str, web_base: &str) -> Self {
        let mut raw = MaybeUninit::<sys::Discord_Client>::uninit();
        // SAFETY: `InitWithBases` fully initialises the handle and copies both strings.
        unsafe {
            sys::Discord_Client_InitWithBases(
                raw.as_mut_ptr(),
                string::borrow(api_base),
                string::borrow(web_base),
            );
            Self {
                raw: raw.assume_init(),
            }
        }
    }

    /// Create a client from an explicit [`ClientCreateOptions`].
    pub fn with_options(options: &mut ClientCreateOptions) -> Self {
        let mut raw = MaybeUninit::<sys::Discord_Client>::uninit();
        // SAFETY: `InitWithOptions` fully initialises the handle; the options are
        // read during the call and not retained.
        unsafe {
            sys::Discord_Client_InitWithOptions(raw.as_mut_ptr(), options.as_raw_mut());
            Self {
                raw: raw.assume_init(),
            }
        }
    }

    /// The application id this client authenticates as.
    pub fn application_id(&self) -> u64 {
        // SAFETY: a plain by-value read of an initialised handle.
        unsafe { sys::Discord_Client_GetApplicationId(self.raw_ptr()) }
    }

    /// Set the application id. Must be called before [`connect`](Self::connect).
    pub fn set_application_id(&mut self, application_id: u64) {
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe { sys::Discord_Client_SetApplicationId(self.as_raw_mut(), application_id) }
    }

    /// Begin connecting to Discord.
    ///
    /// Returns immediately. Progress is reported through
    /// [`on_status_changed`](Self::on_status_changed); the client is only usable
    /// once its status is [`ClientStatus::Ready`].
    pub fn connect(&mut self) {
        // SAFETY: connecting only requires an initialised handle.
        unsafe { sys::Discord_Client_Connect(self.as_raw_mut()) }
    }

    /// Begin disconnecting from Discord.
    ///
    /// Returns immediately; completion is reported as a status change to
    /// [`ClientStatus::Disconnected`].
    pub fn disconnect(&mut self) {
        // SAFETY: disconnecting only requires an initialised handle.
        unsafe { sys::Discord_Client_Disconnect(self.as_raw_mut()) }
    }

    /// The client's current connection status.
    pub fn status(&self) -> ClientStatus {
        // SAFETY: a plain by-value read of an initialised handle.
        ClientStatus::from_raw(unsafe { sys::Discord_Client_GetStatus(self.raw_ptr()) })
    }

    /// The currently authenticated user, or `None` if there is not one.
    ///
    /// Returns `None` before authentication completes rather than a placeholder,
    /// so an unauthenticated client is distinguishable from a real user.
    pub fn current_user(&self) -> Option<User> {
        let mut raw = MaybeUninit::<sys::Discord_UserHandle>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns true,
        // and ownership of the handle transfers to us in that case.
        unsafe {
            sys::Discord_Client_GetCurrentUserV2(self.raw_ptr(), raw.as_mut_ptr())
                .then(|| User::from_raw(raw.assume_init()))
        }
    }

    /// The currently authenticated user, or a placeholder when there is not one.
    ///
    /// The placeholder is a real [`User`] whose fields are all defaults — an id of
    /// `0` and empty strings — which is indistinguishable from a genuine user
    /// without checking. Prefer [`current_user`](Self::current_user).
    #[deprecated(
        note = "returns a dummy user when unauthenticated; use `current_user`, which returns Option"
    )]
    pub fn current_user_or_placeholder(&self) -> User {
        let mut raw = MaybeUninit::<sys::Discord_UserHandle>::uninit();
        // SAFETY: this getter always initialises the handle and transfers ownership.
        unsafe {
            sys::Discord_Client_GetCurrentUser(self.raw_ptr(), raw.as_mut_ptr());
            User::from_raw(raw.assume_init())
        }
    }

    /// Observe connection status changes.
    ///
    /// `error` is `None` while the transition is not a failure. `error_detail`
    /// carries a gateway close code when the SDK has one; see Discord's
    /// [gateway close event codes].
    ///
    /// Replaces any previously registered handler.
    ///
    /// [gateway close event codes]: https://discord.com/developers/docs/topics/opcodes-and-status-codes#gateway-gateway-close-event-codes
    pub fn on_status_changed<F>(&mut self, callback: F)
    where
        F: FnMut(ClientStatus, Option<ClientError>, i32) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            status: sys::Discord_Client_Status,
            error: sys::Discord_Client_Error,
            detail: i32,
            userdata: *mut std::ffi::c_void,
        ) where
            F: FnMut(ClientStatus, Option<ClientError>, i32) + 'static,
        {
            // SAFETY: `userdata` is the boxed `F` installed below and still owned by the SDK.
            unsafe {
                callback::dispatch_mut::<F>(userdata, |f| {
                    let error = match ClientError::from_raw(error) {
                        ClientError::None => None,
                        other => Some(other),
                    };
                    f(ClientStatus::from_raw(status), error, detail)
                })
            }
        }
        // SAFETY: the SDK stores the boxed closure and releases it via `free_fn::<F>()`.
        unsafe {
            sys::Discord_Client_SetStatusChangedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Receive the SDK's log messages at or above `min_severity`.
    ///
    /// Unlike most handlers this one *adds* to the set of log sinks rather than
    /// replacing the previous one.
    ///
    /// # When the closure is released is not guaranteed
    ///
    /// Log sinks are the one callback whose teardown the SDK does not promise,
    /// and the two SDK builds measurably disagree:
    ///
    /// - the **release** library frees the closure once the [`Client`] is dropped
    ///   and the queue is pumped, like any other callback;
    /// - the **debug** library never frees it — not on drop, not on
    ///   [`reset_callbacks`](crate::reset_callbacks) — so it and everything it
    ///   captures live until the process exits.
    ///
    /// Write handlers that are correct either way: keep captures small, and do
    /// not capture credentials or anything whose lifetime you need to control.
    pub fn add_log_callback<F>(&mut self, min_severity: LoggingSeverity, callback: F)
    where
        F: FnMut(&str, LoggingSeverity) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`; the
        // message string is transferred to us and is freed by `take`.
        unsafe {
            sys::Discord_Client_AddLogCallback(
                self.as_raw_mut(),
                Some(log_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
                min_severity.into_raw(),
            )
        }
    }

    /// Receive the voice engine's log messages at or above `min_severity`.
    ///
    /// Adds to the set of voice log sinks rather than replacing it. As with
    /// [`add_log_callback`](Self::add_log_callback), the SDK never releases the
    /// closure — see that method for the consequences.
    pub fn add_voice_log_callback<F>(&mut self, min_severity: LoggingSeverity, callback: F)
    where
        F: FnMut(&str, LoggingSeverity) + 'static,
    {
        // SAFETY: as `add_log_callback`.
        unsafe {
            sys::Discord_Client_AddVoiceLogCallback(
                self.as_raw_mut(),
                Some(log_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
                min_severity.into_raw(),
            )
        }
    }

    /// Write SDK logs at or above `min_severity` to files in `path`.
    ///
    /// Returns `false` if the directory could not be used.
    pub fn set_log_dir(&mut self, path: &str, min_severity: LoggingSeverity) -> bool {
        // SAFETY: the SDK copies the path during the call.
        unsafe {
            sys::Discord_Client_SetLogDir(
                self.as_raw_mut(),
                string::borrow(path),
                min_severity.into_raw(),
            )
        }
    }

    /// Write voice engine logs at or above `min_severity` to files in `path`.
    pub fn set_voice_log_dir(&mut self, path: &str, min_severity: LoggingSeverity) {
        // SAFETY: the SDK copies the path during the call.
        unsafe {
            sys::Discord_Client_SetVoiceLogDir(
                self.as_raw_mut(),
                string::borrow(path),
                min_severity.into_raw(),
            )
        }
    }

    /// Cap how long an HTTP request may take before it is abandoned.
    pub fn set_http_request_timeout(&mut self, timeout: std::time::Duration) {
        let millis = timeout.as_millis().min(i32::MAX as u128) as i32;
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe { sys::Discord_Client_SetHttpRequestTimeout(self.as_raw_mut(), millis) }
    }

    /// Set the OS thread priority for one of the SDK's internal threads.
    pub fn set_thread_priority(&mut self, thread: ClientThread, priority: i32) {
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe {
            sys::Discord_Client_SetThreadPriority(self.as_raw_mut(), thread.into_raw(), priority)
        }
    }

    // ---- Associated helpers (no receiver in the C API) ----

    /// The SDK's major version.
    pub fn version_major() -> i32 {
        // SAFETY: takes no arguments and only reads static version data.
        unsafe { sys::Discord_Client_GetVersionMajor() }
    }

    /// The SDK's minor version.
    pub fn version_minor() -> i32 {
        // SAFETY: as `version_major`.
        unsafe { sys::Discord_Client_GetVersionMinor() }
    }

    /// The SDK's patch version.
    pub fn version_patch() -> i32 {
        // SAFETY: as `version_major`.
        unsafe { sys::Discord_Client_GetVersionPatch() }
    }

    /// The build hash of the SDK binary in use.
    pub fn version_hash() -> String {
        // SAFETY: fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_Client_GetVersionHash(out)) }
    }

    /// The OAuth2 scopes needed for rich presence only.
    pub fn default_presence_scopes() -> String {
        // SAFETY: fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_Client_GetDefaultPresenceScopes(out)) }
    }

    /// The OAuth2 scopes needed for the full communication feature set.
    pub fn default_communication_scopes() -> String {
        // SAFETY: fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_Client_GetDefaultCommunicationScopes(out)) }
    }

    /// The identifier representing "whatever the system default device is".
    pub fn default_audio_device_id() -> String {
        // SAFETY: fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_Client_GetDefaultAudioDeviceId(out)) }
    }

    /// The SDK's own description of a [`ClientStatus`].
    pub fn status_to_string(status: ClientStatus) -> String {
        // SAFETY: fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_Client_StatusToString(status.into_raw(), out)) }
    }

    /// The SDK's own description of a [`ClientError`].
    pub fn error_to_string(error: ClientError) -> String {
        // SAFETY: fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_Client_ErrorToString(error.into_raw(), out)) }
    }

    /// The SDK's own description of a [`ClientThread`].
    pub fn thread_to_string(thread: ClientThread) -> String {
        // SAFETY: fills the out-parameter and transfers ownership of it.
        unsafe { string::out(|out| sys::Discord_Client_ThreadToString(thread.into_raw(), out)) }
    }
}

/// Shared trampoline for the two log-callback registrations, which have
/// identical signatures.
unsafe extern "C" fn log_tramp<F>(
    message: sys::Discord_String,
    severity: sys::Discord_LoggingSeverity,
    userdata: *mut std::ffi::c_void,
) where
    F: FnMut(&str, LoggingSeverity) + 'static,
{
    // SAFETY: `userdata` is the boxed `F`; `message` is transferred to us and freed by `take`.
    unsafe {
        callback::dispatch_mut::<F>(userdata, |f| {
            let text = string::take(message);
            f(&text, LoggingSeverity::from_raw(severity))
        })
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("application_id", &self.application_id())
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}
