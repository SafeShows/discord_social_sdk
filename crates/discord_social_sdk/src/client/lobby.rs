//! Lobbies, guilds, and linking a lobby to a Discord channel.
//!
//! A lobby is a developer-controlled group of users that can chat and talk
//! together. This module holds the [`Client`] half of the lobby API: creating
//! and joining lobbies, looking up [`Lobby`] handles, observing lobby events,
//! and wiring a lobby up to a real Discord channel.
//!
//! # Lobby secrets
//!
//! [`create_or_join_lobby`](Client::create_or_join_lobby) does not take a lobby
//! ID — it takes a *secret*, a hard-to-guess string the game generates. Every
//! user who joins with the same secret lands in the same lobby, and the first
//! one to use it creates that lobby. That is the whole membership mechanism: no
//! server-side component is required, and possession of the secret *is* the
//! invitation.
//!
//! Because of that, a secret should be exchanged only through a channel that
//! already restricts who can see it. Discord's activity-invite and rich-presence
//! systems both carry a secret string visible only to accepted party members,
//! and are the recommended transport.
//!
//! Secrets expire after roughly 30 days. Past that point the old lobby can no
//! longer be joined and the same secret will create a fresh one instead, so this
//! flow is not suitable for long-lived lobbies — use the [server APIs] for those.
//!
//! [server APIs]: https://docs.discord.com/developers/resources/lobby
//!
//! # Lifetime
//!
//! Client-created lobbies auto-delete once idle (no connected users) for a few
//! minutes — 5 by default, configurable up to 7 days through the server API.
//! Only lobbies that have a secret can be left with
//! [`leave_lobby`](Client::leave_lobby); lobbies created through the server API
//! must be manipulated through the server API.
//!
//! # Channel linking
//!
//! A lobby can be linked to a Discord text channel so messages sent in one show
//! up in the other. The flow is: [`user_guilds`](Client::user_guilds) to list
//! the user's servers, [`guild_channels`](Client::guild_channels) to list the
//! channels in the chosen one, then
//! [`link_channel_to_lobby`](Client::link_channel_to_lobby).
//!
//! Only members holding the `CanLinkLobby` flag — settable solely through the
//! server API — may link a lobby, which lets a game restrict linking to clan or
//! guild leaders. Unlinking needs only that same flag, not any Discord
//! permission on the channel.
//!
//! Discord permissions are **not** synchronised into the game: every member of a
//! lobby can read and write the linked channel regardless of whether they could
//! see it on Discord. See
//! [`GuildChannel::is_viewable_and_writeable_by_all_members`] before offering
//! private channels for linking.
//!
//! [`GuildChannel::is_viewable_and_writeable_by_all_members`]:
//!     crate::channel::GuildChannel::is_viewable_and_writeable_by_all_members

use super::Client;
use crate::channel::{Channel, GuildChannel, GuildMinimal};
use crate::error::{Result, to_result};
use crate::lobby::Lobby;
use crate::{callback, span, string};
use discord_social_sdk_sys as sys;
use std::ffi::c_void;
use std::mem::MaybeUninit;

/// Borrow a slice of key/value pairs as two parallel `Discord_String` arrays.
///
/// `Discord_Properties` is `{ size, keys, values }` — two arrays indexed in
/// lockstep, not a map. The returned vectors own nothing but the `Discord_String`
/// descriptors; the bytes they point at still belong to `pairs`.
fn properties_arrays(
    pairs: &[(&str, &str)],
) -> (Vec<sys::Discord_String>, Vec<sys::Discord_String>) {
    let keys = pairs.iter().map(|(k, _)| string::borrow(k)).collect();
    let values = pairs.iter().map(|(_, v)| string::borrow(v)).collect();
    (keys, values)
}

/// Assemble a `Discord_Properties` from arrays produced by [`properties_arrays`].
///
/// # Safety
///
/// The result borrows both vectors and every string they describe. It must not
/// outlive them, and may only be passed to SDK functions that copy their input
/// before returning — which is what `Discord_Client_CreateOrJoinLobbyWithMetadata`
/// does. Never pass the result to `Discord_FreeProperties`: that function frees
/// SDK-allocated buffers, and none of this memory is SDK-allocated.
fn properties(
    keys: &mut [sys::Discord_String],
    values: &mut [sys::Discord_String],
) -> sys::Discord_Properties {
    sys::Discord_Properties {
        size: keys.len().min(values.len()),
        keys: keys.as_mut_ptr(),
        values: values.as_mut_ptr(),
    }
}

/// Trampoline for `Discord_Client_CreateOrJoinLobbyCallback`.
unsafe extern "C" fn create_or_join_tramp<F>(
    result: *mut sys::Discord_ClientResult,
    lobby_id: u64,
    userdata: *mut c_void,
) where
    F: FnOnce(Result<u64>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result).map(|()| lobby_id))) }
}

/// Trampoline for `Discord_Client_LinkOrUnlinkChannelCallback` and
/// `Discord_Client_LeaveLobbyCallback`, which report only success or failure.
unsafe extern "C" fn result_tramp<F>(result: *mut sys::Discord_ClientResult, userdata: *mut c_void)
where
    F: FnOnce(Result<()>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result))) }
}

/// Trampoline for the lobby events that carry only a lobby ID.
unsafe extern "C" fn lobby_event_tramp<F>(lobby_id: u64, userdata: *mut c_void)
where
    F: FnMut(u64) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_mut::<F>(userdata, |f| f(lobby_id)) }
}

/// Trampoline for the lobby events that carry a lobby ID and a member ID.
unsafe extern "C" fn lobby_member_event_tramp<F>(
    lobby_id: u64,
    member_id: u64,
    userdata: *mut c_void,
) where
    F: FnMut(u64, u64) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_mut::<F>(userdata, |f| f(lobby_id, member_id)) }
}

impl Client {
    // ---- Creating, joining, and leaving ----

    /// Join the lobby identified by `secret`, creating it if it does not exist.
    ///
    /// Everyone who joins with the same secret ends up in the same lobby, and
    /// the first to use it creates that lobby; on success the callback receives
    /// the lobby's ID.
    ///
    /// The secret should be hard to guess, since possession of it *is* the
    /// invitation. Discord recommends distributing it through the activity
    /// invite and rich presence systems, which carry a secret string only
    /// accepted party members can see.
    ///
    /// Secrets expire after roughly 30 days, after which the old lobby can no
    /// longer be joined and the same secret creates a new one — so this is not
    /// the right tool for long-lived lobbies. The lobby also auto-deletes once
    /// idle, by default after 5 minutes with no connected user.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn create_or_join_lobby<F>(&mut self, secret: &str, callback: F)
    where
        F: FnOnce(Result<u64>) + 'static,
    {
        // SAFETY: the secret is copied by the SDK during the call, and the SDK
        // owns the boxed closure until it invokes `free_fn`.
        unsafe {
            sys::Discord_Client_CreateOrJoinLobby(
                self.as_raw_mut(),
                string::borrow(secret),
                Some(create_or_join_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// [`create_or_join_lobby`](Self::create_or_join_lobby) with metadata attached.
    ///
    /// `lobby_metadata` is stored on the lobby and `member_metadata` on the
    /// joining member; both are flat string key/value pairs readable by every
    /// member through [`Lobby::metadata`](crate::lobby::Lobby::metadata) and
    /// [`LobbyMember::metadata`](crate::lobby::LobbyMember::metadata). A typical
    /// use is publishing each player's in-game ID so the whole lobby can map
    /// Discord IDs onto game IDs.
    ///
    /// Later calls **overwrite** the lobby's and the member's metadata rather
    /// than merging into it.
    pub fn create_or_join_lobby_with_metadata<F>(
        &mut self,
        secret: &str,
        lobby_metadata: &[(&str, &str)],
        member_metadata: &[(&str, &str)],
        callback: F,
    ) where
        F: FnOnce(Result<u64>) + 'static,
    {
        let (mut lobby_keys, mut lobby_values) = properties_arrays(lobby_metadata);
        let (mut member_keys, mut member_values) = properties_arrays(member_metadata);
        let lobby_props = properties(&mut lobby_keys, &mut lobby_values);
        let member_props = properties(&mut member_keys, &mut member_values);
        // SAFETY: both `Discord_Properties` values borrow the four vectors above
        // and the caller's strings. All of them are still in scope when the call
        // returns, and the SDK copies the properties during the call — the C++
        // wrapper's `ConvertedProperties` frees its own copies as soon as the
        // same call returns, which is what proves the copy happens. They are
        // deliberately not passed to `Discord_FreeProperties`, which is only for
        // SDK-allocated buffers.
        unsafe {
            sys::Discord_Client_CreateOrJoinLobbyWithMetadata(
                self.as_raw_mut(),
                string::borrow(secret),
                lobby_props,
                member_props,
                Some(create_or_join_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Remove the current user from a lobby.
    ///
    /// Only lobbies that have a secret — that is, lobbies created through
    /// [`create_or_join_lobby`](Self::create_or_join_lobby) — can be left.
    /// Lobbies created through the server API are not client-manipulable and
    /// must be changed through the server API too.
    pub fn leave_lobby<F>(&mut self, lobby_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_LeaveLobby(
                self.as_raw_mut(),
                lobby_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Reading lobby state ----

    /// The lobby with the given ID, if the SDK has it loaded.
    pub fn lobby(&self, lobby_id: u64) -> Option<Lobby> {
        let mut raw = MaybeUninit::<sys::Discord_LobbyHandle>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns
        // true, and ownership of the handle then transfers to us.
        unsafe {
            if sys::Discord_Client_GetLobbyHandle(self.raw_ptr(), lobby_id, raw.as_mut_ptr()) {
                Some(Lobby::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The IDs of every lobby the user belongs to that the SDK has loaded.
    ///
    /// Lobbies are loaded optimistically at startup, but may not all be present
    /// the instant the client reaches `Status::Ready`.
    pub fn lobby_ids(&self) -> Vec<u64> {
        // SAFETY: the getter initialises the span and transfers ownership of the
        // backing array; the elements are plain integers needing no wrapping.
        unsafe {
            span::out(
                |out| sys::Discord_Client_GetLobbyIds(self.raw_ptr(), out),
                |id| id,
            )
        }
    }

    /// The channel with the given ID, if the SDK has it loaded.
    ///
    /// Every Discord message is sent in a channel, so the common use is looking
    /// up the channel a message came from. Lobbies work here too: the possible
    /// results are a DM, an ephemeral DM, and a lobby.
    pub fn channel(&self, channel_id: u64) -> Option<Channel> {
        let mut raw = MaybeUninit::<sys::Discord_ChannelHandle>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns
        // true, and ownership of the handle then transfers to us.
        unsafe {
            if sys::Discord_Client_GetChannelHandle(self.raw_ptr(), channel_id, raw.as_mut_ptr()) {
                Some(Channel::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    // ---- Channel linking ----

    /// Every guild (Discord server) the current user is a member of.
    ///
    /// The first step of the channel-linking flow: use it to let the user pick
    /// which server to link to, then call
    /// [`guild_channels`](Self::guild_channels) for that guild.
    pub fn user_guilds<F>(&mut self, callback: F)
    where
        F: FnOnce(Result<Vec<GuildMinimal>>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            guilds: sys::Discord_GuildMinimalSpan,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<Vec<GuildMinimal>>) + 'static,
        {
// SAFETY: the span is transferred to us — the C++ wrapper adopts every
            // element as owned and then frees the array — so the elements are
            // moved out and the array released.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    let outcome = to_result(result).map(|()| {
                        span::take(guilds, |raw| GuildMinimal::from_raw(raw))
                    });
                    f(outcome)
                })
            }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetUserGuilds(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Every channel in `guild_id` that the current user can access.
    ///
    /// Sorted by the channel's `position`, matching the order shown in the
    /// Discord client. Check
    /// [`GuildChannel::is_linkable`](crate::channel::GuildChannel::is_linkable)
    /// before offering a channel for linking.
    pub fn guild_channels<F>(&mut self, guild_id: u64, callback: F)
    where
        F: FnOnce(Result<Vec<GuildChannel>>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            guild_channels: sys::Discord_GuildChannelSpan,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<Vec<GuildChannel>>) + 'static,
        {
// SAFETY: the span is transferred to us — the C++ wrapper adopts every
            // element as owned and then frees the array — so the elements are
            // moved out and the array released.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    let outcome = to_result(result).map(|()| {
                        span::take(guild_channels, |raw| GuildChannel::from_raw(raw))
                    });
                    f(outcome)
                })
            }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetGuildChannels(
                self.as_raw_mut(),
                guild_id,
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Link a Discord channel to a lobby, mirroring messages between the two.
    ///
    /// Requires the current user to hold the `CanLinkLobby` member flag in the
    /// lobby *and* Manage Channels, View Channel and Send Messages in the
    /// channel. A linked lobby is never auto-deleted for idleness.
    pub fn link_channel_to_lobby<F>(&mut self, lobby_id: u64, channel_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_LinkChannelToLobby(
                self.as_raw_mut(),
                lobby_id,
                channel_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Remove any existing channel link from a lobby.
    ///
    /// Any lobby member with the `CanLinkLobby` flag can sever the link; no
    /// Discord permission on the channel itself is needed.
    pub fn unlink_channel_from_lobby<F>(&mut self, lobby_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_UnlinkChannelFromLobby(
                self.as_raw_mut(),
                lobby_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Invite the current user to the guild owning the lobby's linked channel.
    ///
    /// On success the callback receives the invite URL. On desktop the user is
    /// also forwarded to the Discord client to accept or decline it; on consoles
    /// no navigation happens, so the game must present the URL itself.
    ///
    /// `on_provisional_merge_required` fires if the operation cannot proceed
    /// until the user's provisional account is merged into a real Discord
    /// account. It is registered as a persistent handler and stays installed
    /// until the SDK releases it, so it is [`FnMut`] rather than [`FnOnce`].
    pub fn join_linked_lobby_guild<M, F>(
        &mut self,
        lobby_id: u64,
        on_provisional_merge_required: M,
        callback: F,
    ) where
        M: FnMut() + 'static,
        F: FnOnce(Result<String>) + 'static,
    {
        unsafe extern "C" fn merge_tramp<M: FnMut() + 'static>(userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `M` installed below.
            unsafe { callback::dispatch_mut::<M>(userdata, |m| m()) }
        }
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            invite_url: sys::Discord_String,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<String>) + 'static,
        {
            // SAFETY: `invite_url` is transferred to us and must be freed, so it is taken.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    let outcome = to_result(result).map(|()| string::take(invite_url));
                    f(outcome)
                })
            }
        }
        // SAFETY: the SDK owns both boxed closures and frees each through its
        // own `free_fn`; the two userdata pointers are never mixed up because
        // each trampoline is monomorphised over its own closure type.
        unsafe {
            sys::Discord_Client_JoinLinkedLobbyGuild(
                self.as_raw_mut(),
                lobby_id,
                Some(merge_tramp::<M>),
                callback::free_fn::<M>(),
                callback::persistent_userdata(on_provisional_merge_required),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Lobby events ----

    /// Be notified when a lobby becomes available to this client.
    ///
    /// The handler receives the lobby ID. "Available" covers several cases: a
    /// new lobby was created with the current user in it, the current user was
    /// added to an existing lobby, or a lobby recovered after a backend outage.
    ///
    /// It can therefore fire more than once per session for the same lobby —
    /// though never twice in a row, since
    /// [`on_lobby_deleted`](Self::on_lobby_deleted) fires in between.
    pub fn on_lobby_created<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetLobbyCreatedCallback(
                self.as_raw_mut(),
                Some(lobby_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a lobby is no longer available to this client.
    ///
    /// The handler receives the lobby ID. This covers the lobby being deleted,
    /// the current user being removed from it, and backend outages — so the
    /// lobby may well still exist for other users.
    pub fn on_lobby_deleted<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetLobbyDeletedCallback(
                self.as_raw_mut(),
                Some(lobby_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a lobby is edited — for example when its metadata changes.
    ///
    /// The handler receives the lobby ID.
    pub fn on_lobby_updated<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetLobbyUpdatedCallback(
                self.as_raw_mut(),
                Some(lobby_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a user is added to a lobby.
    ///
    /// The handler receives `(lobby_id, member_id)`. It does *not* fire for the
    /// current user — [`on_lobby_created`](Self::on_lobby_created) does that
    /// instead. Membership is separate from connectedness, so a newly added
    /// member is not necessarily online: they have merely gained permission to
    /// connect.
    pub fn on_lobby_member_added<F>(&mut self, callback: F)
    where
        F: FnMut(u64, u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetLobbyMemberAddedCallback(
                self.as_raw_mut(),
                Some(lobby_member_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a member is removed from a lobby and can no longer
    /// connect to it.
    ///
    /// The handler receives `(lobby_id, member_id)`. It does *not* fire for the
    /// current user — [`on_lobby_deleted`](Self::on_lobby_deleted) does that
    /// instead — nor when a member merely quits the game, which surfaces as
    /// [`on_lobby_member_updated`](Self::on_lobby_member_updated) with
    /// [`LobbyMember::connected`](crate::lobby::LobbyMember::connected) now false.
    pub fn on_lobby_member_removed<F>(&mut self, callback: F)
    where
        F: FnMut(u64, u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetLobbyMemberRemovedCallback(
                self.as_raw_mut(),
                Some(lobby_member_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a member of a lobby changes.
    ///
    /// The handler receives `(lobby_id, member_id)`. Fires when the member
    /// connects or disconnects, and when their metadata changes.
    pub fn on_lobby_member_updated<F>(&mut self, callback: F)
    where
        F: FnMut(u64, u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetLobbyMemberUpdatedCallback(
                self.as_raw_mut(),
                Some(lobby_member_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }
}
