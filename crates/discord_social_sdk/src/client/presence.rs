//! Rich presence, activity invites, join requests, and game launch registration.
//!
//! # Rich presence
//!
//! Rich presence is what other users see on Discord while this user plays: the
//! game's name, what they are doing, and how big their party is. Publish it with
//! [`Client::update_rich_presence`] and remove it with
//! [`Client::clear_rich_presence`]. The [`Activity`] type carries the fields; see
//! its documentation for how each one is rendered.
//!
//! Rich presence also powers invites, so an activity that is meant to be joinable
//! must set a party with an id and a size that has room left, and a join secret.
//!
//! # Invites and join requests
//!
//! Both directions are message-backed and both surface through the *same* pair of
//! event callbacks, [`on_activity_invite_created`](Client::on_activity_invite_created)
//! and [`on_activity_invite_updated`](Client::on_activity_invite_updated). The
//! [`invite_type`](crate::activity::ActivityInvite::invite_type) of the delivered
//! invite distinguishes them.
//!
//! Inviting someone in:
//!
//! 1. User A publishes rich presence carrying a join secret.
//! 2. User A calls [`send_activity_invite`](Client::send_activity_invite).
//! 3. User B receives it through `on_activity_invite_created`.
//! 4. User B calls [`accept_activity_invite`](Client::accept_activity_invite) and
//!    gets the join secret back, which the game uses to place them in the party.
//!
//! Asking to be let in:
//!
//! 1. User B calls [`send_activity_join_request`](Client::send_activity_join_request).
//! 2. User A receives an invite of type
//!    [`ActivityActionType::JoinRequest`](crate::enums::ActivityActionType) through
//!    `on_activity_invite_created`.
//! 3. If User A agrees, they call
//!    [`send_activity_join_request_reply`](Client::send_activity_join_request_reply),
//!    which sends User B a regular invite.
//! 4. User B accepts it as above.
//!
//! When the user also runs the Discord desktop client, they may instead accept an
//! invite there. That arrives through
//! [`on_activity_join`](Client::on_activity_join) or
//! [`on_activity_join_with_application`](Client::on_activity_join_with_application)
//! rather than through the invite callbacks.
//!
//! # Launching the game from Discord
//!
//! Accepting an invite in the Discord client only works if Discord knows how to
//! start the game. [`register_launch_command`](Client::register_launch_command) and
//! [`register_launch_steam_application`](Client::register_launch_steam_application)
//! tell it how. **These write registrations outside this process** — into the OS
//! registry or equivalent application association store — so their effect outlives
//! the running program. Call one of them at startup.

use super::Client;
use crate::activity::{Activity, ActivityInvite};
use crate::error::{Result, to_result};
use crate::{callback, string};
use discord_social_sdk_sys as sys;
use std::ffi::c_void;

/// Trampoline shared by the callbacks that report only success or failure.
///
/// Covers `Discord_Client_SendActivityInviteCallback`,
/// `Discord_Client_UpdateRichPresenceCallback`, and
/// `Discord_Client_OpenConnectedGamesSettingsInDiscordCallback`, which are all the
/// same shape.
unsafe extern "C" fn result_tramp<F>(result: *mut sys::Discord_ClientResult, userdata: *mut c_void)
where
    F: FnOnce(Result<()>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result))) }
}

/// Trampoline shared by both `Discord_Client_ActivityInviteCallback` sites.
///
/// The invite is **transferred** to the callback, not lent: the official C++
/// wrapper adopts it as `DiscordObjectState::Owned` and lets its destructor run
/// when the handler returns. It is therefore taken with `from_raw` and dropped
/// here; borrowing it instead would leak one invite per event.
///
/// The handler still receives it by reference so the common case — reading a few
/// fields — needs no clone. Handlers that must keep the invite can clone it.
unsafe extern "C" fn invite_tramp<F>(invite: *mut sys::Discord_ActivityInvite, userdata: *mut c_void)
where
    F: FnMut(&ActivityInvite) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed below. `invite` is an owned
    // handle transferred to us; reading it out by value and wrapping it in
    // `ActivityInvite` makes the local responsible for dropping it.
    unsafe {
        callback::dispatch_mut::<F>(userdata, |f| {
            let owned = ActivityInvite::from_raw(std::ptr::read(invite));
            f(&owned)
        })
    }
}

impl Client {
    // ---- Rich presence ----

    /// Publish rich presence for the current user.
    ///
    /// Other users on Discord then see that this user is playing, along with
    /// whatever hints the [`Activity`] carries — a character name, a map, a score.
    /// Rich presence is also what makes activity invites possible.
    ///
    /// The activity is a *partial* object: `name` and `application_id` cannot be
    /// set here and are overwritten by the SDK.
    ///
    /// On desktop this may be called before [`connect`](Client::connect), but
    /// connecting clears it; while disconnected it sets the presence directly in
    /// the user's Discord client, when one is available.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn update_rich_presence<F>(&mut self, activity: &mut Activity, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: `activity` is read during the call and not retained; the SDK owns
        // the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_UpdateRichPresence(
                self.as_raw_mut(),
                activity.as_raw_mut(),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Clear the rich presence for the current user.
    pub fn clear_rich_presence(&mut self) {
        // SAFETY: only requires an initialised handle.
        unsafe { sys::Discord_Client_ClearRichPresence(self.as_raw_mut()) }
    }

    // ---- Sending invites and join requests ----

    /// Send an activity invite to `user_id`.
    ///
    /// The invite travels as a Discord message, so it only goes through when at
    /// least one of the following holds:
    ///
    /// - both users are online, in the game, and have not blocked each other;
    /// - both users are friends;
    /// - both users share a Discord server and have previously DM'd each other.
    ///
    /// `content` is optional message text to accompany the invite; an empty string
    /// is fine.
    pub fn send_activity_invite<F>(&mut self, user_id: u64, content: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the content string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_SendActivityInvite(
                self.as_raw_mut(),
                user_id,
                string::borrow(content),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Ask `user_id` to be let into their activity.
    ///
    /// Callable whenever that user has a rich presence activity for this game with
    /// room for another member. They receive an activity invite which they can
    /// accept or reject.
    pub fn send_activity_join_request<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SendActivityJoinRequest(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Approve a join request, letting the requesting user in.
    ///
    /// `invite` is the join-request invite delivered by
    /// [`on_activity_invite_created`](Self::on_activity_invite_created). This sends
    /// the original user an activity invite, which they must then accept.
    pub fn send_activity_join_request_reply<F>(&mut self, invite: &ActivityInvite, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the C signature takes a non-const `Discord_ActivityInvite*` but
        // only reads it, matching the const-cast the official C++ wrapper performs
        // at this same call site. The invite is not retained past the call.
        unsafe {
            sys::Discord_Client_SendActivityJoinRequestReply(
                self.as_raw_mut(),
                invite.raw_ptr(),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Accept an activity invite this user received.
    ///
    /// The callback receives the join secret from the inviting user's rich presence
    /// activity, which the game uses to place this user into its own party system —
    /// for example by passing it as the secret when creating or joining a lobby.
    ///
    /// `invite` may be borrowed straight from
    /// [`on_activity_invite_created`](Self::on_activity_invite_created).
    pub fn accept_activity_invite<F>(&mut self, invite: &ActivityInvite, callback: F)
    where
        F: FnOnce(Result<String>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            join_secret: sys::Discord_String,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<String>) + 'static,
        {
            // SAFETY: `join_secret` is transferred to us and must be freed,
            // so it is viewed rather than taken.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    let outcome = to_result(result).map(|()| string::take(join_secret));
                    f(outcome)
                })
            }
        }
        // SAFETY: the C signature takes a non-const `Discord_ActivityInvite*` but
        // only reads it, matching the const-cast the official C++ wrapper performs
        // at this same call site. The invite is not retained past the call.
        unsafe {
            sys::Discord_Client_AcceptActivityInvite(
                self.as_raw_mut(),
                invite.raw_ptr(),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Invite and join events ----

    /// Be notified when this user receives an activity invite.
    ///
    /// Invites are sent as Discord messages, and the SDK parses them out: the
    /// message-created callback does *not* fire for them. The delivered invite
    /// carries everything needed to identify it later for
    /// [`accept_activity_invite`](Self::accept_activity_invite).
    ///
    /// Join requests arrive here too, distinguished by
    /// [`ActivityActionType::JoinRequest`](crate::enums::ActivityActionType).
    ///
    /// The invite is **borrowed**: it belongs to the SDK and is freed as soon as
    /// the handler returns. To keep it, clone it out with
    /// [`Clone::clone`].
    pub fn on_activity_invite_created<F>(&mut self, callback: F)
    where
        F: FnMut(&ActivityInvite) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetActivityInviteCreatedCallback(
                self.as_raw_mut(),
                Some(invite_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when an activity invite this user already holds changes.
    ///
    /// The only thing that currently changes is validity: an invite stops being
    /// joinable when the sender goes offline or leaves the party, and can become
    /// joinable again if they rejoin. See
    /// [`ActivityInvite::is_valid`](crate::activity::ActivityInvite::is_valid).
    ///
    /// The invite is **borrowed** on the same terms as
    /// [`on_activity_invite_created`](Self::on_activity_invite_created).
    pub fn on_activity_invite_updated<F>(&mut self, callback: F)
    where
        F: FnMut(&ActivityInvite) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetActivityInviteUpdatedCallback(
                self.as_raw_mut(),
                Some(invite_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when this user accepts an activity invite inside the Discord client.
    ///
    /// Fires when the user is also running Discord on their computer and accepts
    /// there rather than in-game. The handler receives the join secret from the
    /// activity's rich presence, which the game uses to join them to its own party
    /// system.
    pub fn on_activity_join<F>(&mut self, callback: F)
    where
        F: FnMut(&str) + 'static,
    {
        unsafe extern "C" fn tramp<F>(join_secret: sys::Discord_String, userdata: *mut c_void)
        where
            F: FnMut(&str) + 'static,
        {
            // SAFETY: `userdata` is the boxed `F` installed below; `join_secret` is
            // transferred to us and must be freed, so it is taken.
            unsafe {
                callback::dispatch_mut::<F>(userdata, |f| {
                    let secret = string::take(join_secret);
                    f(&secret)
                })
            }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetActivityJoinCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// As [`on_activity_join`](Self::on_activity_join), but also reporting which
    /// application the join was for.
    ///
    /// The handler receives `(application_id, join_secret)`.
    pub fn on_activity_join_with_application<F>(&mut self, callback: F)
    where
        F: FnMut(u64, &str) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            application_id: u64,
            join_secret: sys::Discord_String,
            userdata: *mut c_void,
        ) where
            F: FnMut(u64, &str) + 'static,
        {
            // SAFETY: `userdata` is the boxed `F` installed below; `join_secret` is
            // transferred to us and must be freed, so it is taken.
            unsafe {
                callback::dispatch_mut::<F>(userdata, |f| {
                    let secret = string::take(join_secret);
                    f(application_id, &secret)
                })
            }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetActivityJoinWithApplicationCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    // ---- Launch registration and app integration ----

    /// Tell Discord how to launch this game, so invites accepted in the Discord
    /// client can start it.
    ///
    /// Returns whether the command was registered.
    ///
    /// **This registration lives outside the process.** It is written to the
    /// operating system's application registry, so it persists after the game
    /// exits and affects how Discord launches the game in future sessions. Call it
    /// when the SDK starts up.
    ///
    /// On Windows and Linux `command` is a path to an executable and may include
    /// launch parameters, for example `"C:\path\to my\game.exe" --full-screen`.
    /// Passing an empty string registers the currently running executable. To
    /// launch through a custom protocol such as `my-awesome-game://`, pass that as
    /// an argument of the executable that handles it.
    ///
    /// On macOS the game must be bundled — `command` must be a `.app`, or a custom
    /// protocol, but *not* a path to a bare executable. An empty string registers
    /// the currently running bundle, if any.
    pub fn register_launch_command(&mut self, application_id: u64, command: &str) -> bool {
        // SAFETY: the command string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_RegisterLaunchCommand(
                self.as_raw_mut(),
                application_id,
                string::borrow(command),
            )
        }
    }

    /// Tell Discord this game is a Steam application, so invites accepted in the
    /// Discord client can launch it through Steam.
    ///
    /// Returns whether the registration succeeded. Like
    /// [`register_launch_command`](Self::register_launch_command) this writes an
    /// association outside the process and should be called at SDK startup.
    pub fn register_launch_steam_application(
        &mut self,
        application_id: u64,
        steam_app_id: u32,
    ) -> bool {
        // SAFETY: only requires an initialised handle and by-value arguments.
        unsafe {
            sys::Discord_Client_RegisterLaunchSteamApplication(
                self.as_raw_mut(),
                application_id,
                steam_app_id,
            )
        }
    }

    /// Point the Discord overlay at a different process's window.
    ///
    /// The SDK uses Discord's overlay to keep account linking in-game. If the
    /// game's main window belongs to a different process than the one running the
    /// integration, set that process's pid here. Defaults to the current pid.
    pub fn set_game_window_pid(&mut self, pid: i32) {
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe { sys::Discord_Client_SetGameWindowPid(self.as_raw_mut(), pid) }
    }

    /// Open the Connected Games settings page in the Discord client.
    ///
    /// That is where users manage their settings for games using the Discord Social
    /// SDK. Does nothing if the client is not connected or the user is on a
    /// provisional account, and is always a no-op on console platforms.
    pub fn open_connected_games_settings_in_discord<F>(&mut self, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_OpenConnectedGamesSettingsInDiscord(
                self.as_raw_mut(),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Check whether the Discord mobile app is installed on this device.
    ///
    /// Always reports `false` on desktop platforms. Requires no connection and may
    /// be called at any time — useful for deciding whether to offer app-based
    /// authorization or fall back to a browser flow.
    ///
    /// Platform requirements:
    ///
    /// - iOS: the app's `Info.plist` must list `"discord"` in
    ///   `LSApplicationQueriesSchemes`.
    /// - Android: the manifest must list `"com.discord"` in its `queries` element
    ///   (required on Android 11 and later).
    ///
    /// Despite being a local check this is delivered asynchronously, through
    /// [`run_callbacks`](crate::run_callbacks).
    pub fn is_discord_app_installed<F>(&mut self, callback: F)
    where
        F: FnOnce(bool) + 'static,
    {
        unsafe extern "C" fn tramp<F>(installed: bool, userdata: *mut c_void)
        where
            F: FnOnce(bool) + 'static,
        {
            // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
            unsafe { callback::dispatch_once::<F>(userdata, |f| f(installed)) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_IsDiscordAppInstalled(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }
}
