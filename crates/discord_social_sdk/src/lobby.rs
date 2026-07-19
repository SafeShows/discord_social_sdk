//! Lobbies and lobby membership.
//!
//! A lobby is an arbitrary, developer-controlled group of users that can
//! communicate with each other. Lobbies are created and managed either through
//! Discord's [server APIs] or, without any server-side component, through
//! `Client::create_or_join_lobby`.
//!
//! [server APIs]: https://docs.discord.com/developers/resources/lobby
//!
//! # Metadata
//!
//! Both [`Lobby`] and [`LobbyMember`] carry developer-supplied metadata: a flat
//! set of string key/value pairs. The SDK returns it as a `Discord_Properties`
//! parallel array whose buffers we must release with `Discord_FreeProperties`;
//! this module copies every key and value into an owned [`HashMap`] and frees
//! the SDK's memory before returning.

use std::collections::HashMap;
use std::mem::MaybeUninit;

use discord_social_sdk_sys as sys;

use crate::call::CallInfo;
use crate::channel::LinkedChannel;
use crate::handle::handle;
use crate::user::User;
use crate::{span, string};

/// Copy a `Discord_Properties` value into an owned map and release the SDK's
/// buffers.
///
/// The SDK allocates the key and value arrays *and* every string inside them.
/// `Discord_FreeProperties` releases all of it in one go, so the strings are
/// read with a borrowing copy rather than by taking ownership, which would
/// double free.
///
/// # Safety
///
/// `raw` must be a fully initialised `Discord_Properties` whose ownership the
/// SDK just transferred to us. Calling this twice on the same value double frees.
unsafe fn take_properties(raw: sys::Discord_Properties) -> HashMap<String, String> {
    let mut out = HashMap::with_capacity(raw.size);
    if !raw.keys.is_null() && !raw.values.is_null() {
        for i in 0..raw.size {
            // SAFETY: the SDK guarantees both arrays hold `raw.size` initialised
            // strings, each valid until `Discord_FreeProperties` runs below.
            let (key, value) = unsafe {
                (
                    string::view(*raw.keys.add(i)),
                    string::view(*raw.values.add(i)),
                )
            };
            out.insert(key, value);
        }
    }
    // SAFETY: `raw` came from the SDK and has not been freed yet.
    unsafe { sys::Discord_FreeProperties(raw) };
    out
}

/// Call an SDK getter of the shape `void Get(self, Discord_Properties* out)`.
///
/// # Safety
///
/// `f` must fully initialise the out-parameter and transfer ownership of it.
unsafe fn properties_out<F>(f: F) -> HashMap<String, String>
where
    F: FnOnce(*mut sys::Discord_Properties),
{
    let mut raw = MaybeUninit::<sys::Discord_Properties>::uninit();
    f(raw.as_mut_ptr());
    // SAFETY: `f` initialised the out-parameter per this function's contract.
    unsafe { take_properties(raw.assume_init()) }
}

handle! {
    /// A single member of a [`Lobby`].
    ///
    /// Members are not removed automatically when they close the game or
    /// temporarily disconnect, so a member may exist while
    /// [`connected`](LobbyMember::connected) is `false`.
    LobbyMember(sys::Discord_LobbyMemberHandle) {
        drop: sys::Discord_LobbyMemberHandle_Drop,
        clone: sys::Discord_LobbyMemberHandle_Clone,
    }
}

impl LobbyMember {
    /// Whether this user is allowed to link a channel to the lobby.
    ///
    /// Under the hood this checks the `CanLinkLobby` member flag, which can only
    /// be set through the server API's `add_lobby_member`. It exists so a game
    /// can restrict channel linking to, say, the clan or guild leader.
    pub fn can_link_lobby(&self) -> bool {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LobbyMemberHandle_CanLinkLobby(self.raw_ptr()) }
    }

    /// Whether the user is currently connected to the lobby.
    pub fn connected(&self) -> bool {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LobbyMemberHandle_Connected(self.raw_ptr()) }
    }

    /// The user ID of the lobby member.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LobbyMemberHandle_Id(self.raw_ptr()) }
    }

    /// Developer-supplied metadata for this member.
    ///
    /// A common use is to store the game's internal user ID alongside the
    /// Discord one, so every member of a lobby knows the mapping for each user.
    pub fn metadata(&self) -> HashMap<String, String> {
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of the properties to us.
        unsafe { properties_out(|out| sys::Discord_LobbyMemberHandle_Metadata(self.raw_ptr(), out)) }
    }

    /// The full user record for this member, if the SDK has one.
    pub fn user(&self) -> Option<User> {
        let mut raw = MaybeUninit::<sys::Discord_UserHandle>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns
        // true, and ownership of the handle then transfers to us.
        unsafe {
            if sys::Discord_LobbyMemberHandle_User(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(User::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }
}

impl std::fmt::Debug for LobbyMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LobbyMember")
            .field("id", &self.id())
            .field("connected", &self.connected())
            .field("can_link_lobby", &self.can_link_lobby())
            .finish()
    }
}

handle! {
    /// A lobby: a developer-controlled group of users that can chat and talk
    /// together.
    ///
    /// # Lifetime
    ///
    /// Lobbies are ephemeral. One that has been idle — no user actively
    /// connected — for its auto-delete timeout (5 minutes by default, up to 7
    /// days when configured through the server API) is removed. A lobby linked
    /// to a Discord channel is never auto-deleted.
    ///
    /// # Limits
    ///
    /// A lobby may hold at most 1,000 members, and each user may be in at most
    /// 200 lobbies per game. Voice calls should stay far below that ceiling —
    /// Discord recommends around 25 participants or fewer.
    ///
    /// # Channel linking
    ///
    /// A lobby can be linked to a Discord channel so that messages sent in one
    /// place appear in the other; see [`linked_channel`](Lobby::linked_channel)
    /// and [`LobbyMember::can_link_lobby`]. Because Discord permissions are not
    /// synchronised into the game, *every* lobby member can read and write the
    /// linked channel regardless of their Discord permissions — see
    /// [`GuildChannel::is_viewable_and_writeable_by_all_members`].
    ///
    /// [`GuildChannel::is_viewable_and_writeable_by_all_members`]:
    ///     crate::channel::GuildChannel::is_viewable_and_writeable_by_all_members
    ///
    /// # Staleness
    ///
    /// A handle references both the underlying data and the SDK instance.
    /// Updates to the data are visible through existing handles without
    /// re-creating them. If the SDK instance is destroyed while a handle is
    /// still alive, every accessor returns a default value instead.
    Lobby(sys::Discord_LobbyHandle) {
        drop: sys::Discord_LobbyHandle_Drop,
        clone: sys::Discord_LobbyHandle_Clone,
    }
}

impl Lobby {
    /// Information about the active voice call in this lobby, if there is one.
    ///
    /// Available even when the current user has not joined the call, so the
    /// game can show which members are already in voice.
    pub fn call_info(&self) -> Option<CallInfo> {
        let mut raw = MaybeUninit::<sys::Discord_CallInfoHandle>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns
        // true, and ownership of the handle then transfers to us.
        unsafe {
            if sys::Discord_LobbyHandle_GetCallInfoHandle(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(CallInfo::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The member with the given user ID, if they belong to this lobby.
    pub fn member(&self, member_id: u64) -> Option<LobbyMember> {
        let mut raw = MaybeUninit::<sys::Discord_LobbyMemberHandle>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns
        // true, and ownership of the handle then transfers to us.
        unsafe {
            if sys::Discord_LobbyHandle_GetLobbyMemberHandle(
                self.raw_ptr(),
                member_id,
                raw.as_mut_ptr(),
            ) {
                Some(LobbyMember::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The ID of the lobby.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LobbyHandle_Id(self.raw_ptr()) }
    }

    /// The Discord channel this lobby is linked to, if any.
    pub fn linked_channel(&self) -> Option<LinkedChannel> {
        let mut raw = MaybeUninit::<sys::Discord_LinkedChannel>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns
        // true, and ownership of the value then transfers to us.
        unsafe {
            if sys::Discord_LobbyHandle_LinkedChannel(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(LinkedChannel::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The user IDs of every member of this lobby.
    ///
    /// Cheaper than [`members`](Lobby::members) when only the IDs are needed.
    pub fn member_ids(&self) -> Vec<u64> {
        // SAFETY: the getter initialises the span and transfers ownership of the
        // backing array; the elements are plain integers needing no wrapping.
        unsafe {
            span::out(
                |out| sys::Discord_LobbyHandle_LobbyMemberIds(self.raw_ptr(), out),
                |id| id,
            )
        }
    }

    /// A handle for every member of this lobby.
    pub fn members(&self) -> Vec<LobbyMember> {
        // SAFETY: the getter initialises the span and transfers ownership of
        // both the array and every handle in it.
        unsafe {
            span::out(
                |out| sys::Discord_LobbyHandle_LobbyMembers(self.raw_ptr(), out),
                |raw| LobbyMember::from_raw(raw),
            )
        }
    }

    /// Developer-supplied metadata for this lobby.
    ///
    /// Simple string key/value pairs, a way to associate internal game state
    /// with the lobby so that every member has easy access to it.
    pub fn metadata(&self) -> HashMap<String, String> {
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of the properties to us.
        unsafe { properties_out(|out| sys::Discord_LobbyHandle_Metadata(self.raw_ptr(), out)) }
    }
}

impl std::fmt::Debug for Lobby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lobby")
            .field("id", &self.id())
            .field("member_ids", &self.member_ids())
            .finish()
    }
}
