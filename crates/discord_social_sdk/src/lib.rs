//! Safe Rust bindings to the [Discord Social SDK].
//!
//! This crate wraps the SDK's C API with ownership-correct Rust types: handles
//! become RAII structs, `Discord_String` becomes [`String`], optional getters
//! become [`Option`], and `ClientResult` becomes [`Result`].
//!
//! [Discord Social SDK]: https://discord.com/developers/docs/discord-social-sdk/overview
//!
//! # Driving the SDK
//!
//! The SDK is single-threaded and callback-driven. Nothing is delivered until
//! [`run_callbacks`] is called, which must happen regularly from the same thread
//! that created the [`Client`] — typically once per frame:
//!
//! ```no_run
//! use discord_social_sdk::{Client, run_callbacks};
//!
//! let mut client = Client::new();
//! client.set_application_id(1234567890);
//! client.on_status_changed(|status, _err, _detail| {
//!     println!("status: {status:?}");
//! });
//! client.connect();
//!
//! loop {
//!     run_callbacks();
//!     std::thread::sleep(std::time::Duration::from_millis(16));
//! }
//! ```
//!
//! # Threading
//!
//! Handle types are deliberately neither [`Send`] nor [`Sync`]. The SDK expects
//! to be driven from one thread, and callbacks are delivered on whichever thread
//! calls [`run_callbacks`].
//!
//! # Coverage
//!
//! 502 of the C API's 513 exported functions are wrapped here. The 11 that are
//! not are omitted on purpose:
//!
//! - `Discord_Alloc` — the SDK's allocator. Nothing in a Rust program should be
//!   allocating SDK memory; the wrapper only ever *frees* what the SDK returns.
//! - `Discord_ClientResult_Set*`, `_Clone`, `_ToString` — a `ClientResult` is an
//!   inbound error report, converted to [`Error`] the moment it arrives.
//!   Constructing or mutating one has no meaning for a caller.
//!
//! Everything else is reachable through this crate. If you need one of the above,
//! it is still available raw in [`sys`].

#![warn(missing_docs)]

pub use discord_social_sdk_sys as sys;

mod callback;
mod handle;
mod span;
mod string;

pub mod activity;
pub mod audio;
pub mod auth;
pub mod call;
pub mod channel;
pub mod client;
pub mod enums;
pub mod error;
pub mod lobby;
pub mod message;
pub mod relationship;
pub mod user;

pub use activity::{
    Activity, ActivityAssets, ActivityButton, ActivityInvite, ActivityParty, ActivitySecrets,
    ActivityTimestamps,
};
pub use audio::{AudioDevice, VadThresholdSettings};
pub use auth::{
    AuthorizationArgs, AuthorizationCodeChallenge, AuthorizationCodeVerifier,
    DeviceAuthorizationArgs,
};
pub use call::{Call, CallInfo, VoiceState};
pub use channel::{Channel, GuildChannel, GuildMinimal, LinkedChannel, LinkedLobby};
pub use client::{AuthorizationCode, Client, ClientCreateOptions, CurrentUserInfo, Token};
pub use error::{Error, ErrorKind, Result};
pub use lobby::{Lobby, LobbyMember};
pub use message::{AdditionalContent, Message, UserMessageSummary};
pub use relationship::Relationship;
pub use user::{User, UserApplicationProfile};

/// Deliver all callbacks queued since the last call.
///
/// The SDK does not run its own event loop. Nothing — connection status changes,
/// completed requests, incoming messages — is observed until this runs, so it
/// belongs in the host application's main loop.
///
/// Call it from the same thread throughout; that thread is where every callback
/// registered through this crate will run.
pub fn run_callbacks() {
    unsafe { sys::Discord_RunCallbacks() }
}

/// Allow SDK objects to be used from multiple threads.
///
/// Opts the SDK into its thread-safe mode. This crate's types are still `!Send`
/// and `!Sync`, so this only matters when reaching into [`sys`] directly.
///
/// Call before creating any [`Client`].
pub fn set_free_threaded() {
    unsafe { sys::Discord_SetFreeThreaded() }
}

/// Drop every callback the SDK is currently holding.
///
/// Releases userdata for all registered callbacks. Useful at shutdown to ensure
/// closures are freed before the objects they capture.
pub fn reset_callbacks() {
    unsafe { sys::Discord_ResetCallbacks() }
}
