//! Friends, blocking, and the users behind them.
//!
//! # Discord friends versus game friends
//!
//! The SDK maintains two independent relationship graphs, and almost every
//! operation here exists in both flavours:
//!
//! - **Discord friends** are real friendships on the user's Discord account.
//!   They persist across every game and are visible in the Discord client
//!   itself. Both users see each other's presence regardless of what they are
//!   playing.
//! - **Game friends** are scoped to your application. They do not carry over to
//!   other games, and the two users only see each other's online state while
//!   playing a game in which they are friends.
//!
//! Sending the wrong kind is a visible mistake to the player, so pick
//! deliberately: [`send_discord_friend_request`](Client::send_discord_friend_request)
//! asks for a friendship on Discord proper, while
//! [`send_game_friend_request`](Client::send_game_friend_request) creates one
//! that only exists inside your game.
//!
//! A single [`Relationship`] carries both types at once — see the
//! [relationship module](crate::relationship) for how they interact when a game
//! friend is later upgraded to a Discord friend.
//!
//! # Lifecycle
//!
//! 1. One side sends a request. Their side of the relationship becomes
//!    [`PendingOutgoing`](crate::enums::RelationshipType::PendingOutgoing) and
//!    the other side's becomes
//!    [`PendingIncoming`](crate::enums::RelationshipType::PendingIncoming).
//! 2. The recipient calls one of the `accept_*` or `reject_*` methods; the
//!    sender may instead `cancel_*` while it is still pending. Accepting moves
//!    both sides to [`Friend`](crate::enums::RelationshipType::Friend).
//! 3. Friendships end through
//!    [`remove_game_friend`](Client::remove_game_friend) (game only) or
//!    [`remove_discord_and_game_friend`](Client::remove_discord_and_game_friend)
//!    (both graphs at once).
//!
//! Sending a request to someone who already has a pending *incoming* request
//! from you simply completes the friendship, so a "send" button doubles as an
//! "accept" button.
//!
//! Blocking sits outside both graphs: [`block_user`](Client::block_user) removes
//! any existing relationship and applies everywhere, on Discord and in every
//! game. [`unblock_user`](Client::unblock_user) does not restore what was
//! removed.
//!
//! # Consent
//!
//! None of these mutations should ever happen without a deliberate user action.
//! Never send, accept, or remove friendships on the player's behalf.
//!
//! # Staying up to date
//!
//! [`on_relationship_groups_updated`](Client::on_relationship_groups_updated) is
//! the callback to drive a friends list from: rebuild it with
//! [`relationships_by_group`](Client::relationships_by_group) whenever it fires.

use super::Client;
use crate::enums::{RelationshipGroupType, StatusType};
use crate::error::{Result, to_result};
use crate::relationship::Relationship;
use crate::user::User;
use crate::{callback, span, string};
use discord_social_sdk_sys as sys;
use std::ffi::c_void;
use std::mem::MaybeUninit;

/// Trampoline shared by every relationship mutation, all of which report only
/// success or failure.
///
/// Covers `Discord_Client_UpdateRelationshipCallback`,
/// `Discord_Client_SendFriendRequestCallback` and
/// `Discord_Client_UpdateStatusCallback`, which are identical in shape.
unsafe extern "C" fn result_tramp<F>(result: *mut sys::Discord_ClientResult, userdata: *mut c_void)
where
    F: FnOnce(Result<()>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result))) }
}

/// Trampoline shared by every persistent event that reports only a user id.
unsafe extern "C" fn user_id_tramp<F>(user_id: u64, userdata: *mut c_void)
where
    F: FnMut(u64) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_mut::<F>(userdata, |f| f(user_id)) }
}

/// Trampoline shared by the relationship created and deleted events.
unsafe extern "C" fn relationship_event_tramp<F>(
    user_id: u64,
    is_discord_relationship: bool,
    userdata: *mut c_void,
) where
    F: FnMut(u64, bool) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_mut::<F>(userdata, |f| f(user_id, is_discord_relationship)) }
}

impl Client {
    // ---- Sending friend requests ----

    /// Send a **Discord** friend request to a user identified by username.
    ///
    /// `username` is the target's globally unique Discord username, *not* their
    /// display name.
    ///
    /// On success the current user's Discord relationship becomes
    /// [`PendingOutgoing`](crate::enums::RelationshipType::PendingOutgoing) and
    /// the target's becomes
    /// [`PendingIncoming`](crate::enums::RelationshipType::PendingIncoming). If
    /// the target had already sent the current user a Discord friend request,
    /// the two become Discord friends instead.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn send_discord_friend_request<F>(&mut self, username: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the username is copied by the SDK during the call; the boxed
        // closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_SendDiscordFriendRequest(
                self.as_raw_mut(),
                string::borrow(username),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Send a **Discord** friend request to a user identified by id.
    ///
    /// Behaves exactly like
    /// [`send_discord_friend_request`](Self::send_discord_friend_request), but
    /// takes the target's Discord user id rather than their username.
    pub fn send_discord_friend_request_by_id<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_SendDiscordFriendRequestById(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Send — or accept — a **game** friend request, by username.
    ///
    /// The friendship this creates exists only inside your application; it does
    /// not appear on Discord and does not carry over to other games. Use
    /// [`send_discord_friend_request`](Self::send_discord_friend_request) for a
    /// real Discord friendship.
    ///
    /// `username` is the target's globally unique Discord username, not their
    /// display name. If the target had already sent the current user a game
    /// friend request, this accepts it and the two become game friends.
    pub fn send_game_friend_request<F>(&mut self, username: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the username is copied by the SDK during the call; the boxed
        // closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_SendGameFriendRequest(
                self.as_raw_mut(),
                string::borrow(username),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Send — or accept — a **game** friend request, by user id.
    ///
    /// Behaves exactly like
    /// [`send_game_friend_request`](Self::send_game_friend_request), but takes
    /// the target's Discord user id rather than their username.
    pub fn send_game_friend_request_by_id<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_SendGameFriendRequestById(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Answering friend requests ----

    /// Accept an incoming **Discord** friend request.
    ///
    /// Fails unless the target's Discord relationship with the current user is
    /// [`PendingIncoming`](crate::enums::RelationshipType::PendingIncoming).
    pub fn accept_discord_friend_request<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_AcceptDiscordFriendRequest(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Accept an incoming **game** friend request.
    ///
    /// Fails unless the game relationship with the target user is
    /// [`PendingIncoming`](crate::enums::RelationshipType::PendingIncoming).
    pub fn accept_game_friend_request<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_AcceptGameFriendRequest(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Decline an incoming **Discord** friend request.
    ///
    /// Fails unless the Discord relationship with the target user is
    /// [`PendingIncoming`](crate::enums::RelationshipType::PendingIncoming).
    pub fn reject_discord_friend_request<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_RejectDiscordFriendRequest(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Decline an incoming **game** friend request.
    ///
    /// Fails unless the game relationship with the target user is
    /// [`PendingIncoming`](crate::enums::RelationshipType::PendingIncoming).
    pub fn reject_game_friend_request<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_RejectGameFriendRequest(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Withdraw an outgoing **Discord** friend request.
    ///
    /// Fails unless the Discord relationship with the target user is
    /// [`PendingOutgoing`](crate::enums::RelationshipType::PendingOutgoing).
    pub fn cancel_discord_friend_request<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_CancelDiscordFriendRequest(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Withdraw an outgoing **game** friend request.
    ///
    /// Fails unless the game relationship with the target user is
    /// [`PendingOutgoing`](crate::enums::RelationshipType::PendingOutgoing).
    pub fn cancel_game_friend_request<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_CancelGameFriendRequest(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Ending relationships ----

    /// Remove **both** the Discord friendship and the game friendship with a user.
    ///
    /// This affects the user's real Discord friends list, not just your game.
    /// Use [`remove_game_friend`](Self::remove_game_friend) to end only the
    /// in-game friendship.
    ///
    /// Fails if the target is neither a Discord nor a game friend.
    pub fn remove_discord_and_game_friend<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_RemoveDiscordAndGameFriend(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Remove only the **game** friendship with a user.
    ///
    /// Any Discord friendship between the two is left untouched.
    ///
    /// Fails if the target is not currently a game friend.
    pub fn remove_game_friend<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_RemoveGameFriend(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Block a user.
    ///
    /// A blocked user can no longer send the current user friend requests,
    /// activity invites, or messages. Blocking also removes any existing
    /// relationship between the two, and applies everywhere — blocking someone
    /// in one game blocks them on Discord and in every other game as well.
    pub fn block_user<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_BlockUser(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Unblock a user.
    ///
    /// Does not restore whatever relationship blocking removed; the two are
    /// simply strangers again.
    ///
    /// Fails if the target user is not currently blocked.
    pub fn unblock_user<F>(&mut self, user_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_UnblockUser(
                self.as_raw_mut(),
                user_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Reading the relationship list ----

    /// Every relationship the current user has.
    ///
    /// Includes all Discord relationships plus all game relationships for the
    /// current game.
    ///
    /// This is a direct, synchronous read of the SDK's local state — it does not
    /// hit the network and does not wait for
    /// [`run_callbacks`](crate::run_callbacks).
    pub fn relationships(&self) -> Vec<Relationship> {
        // SAFETY: read-only span getter. The out-parameter transfers ownership of
        // both the array and every handle in it, so each element is adopted by a
        // `Relationship` and `span::out` frees the backing array. (Spans handed
        // *to* a callback are borrowed instead; this one is not.)
        unsafe {
            span::out(
                |out| sys::Discord_Client_GetRelationships(self.raw_ptr(), out),
                |raw| Relationship::from_raw(raw),
            )
        }
    }

    /// The relationships in one display group.
    ///
    /// Relationships are partitioned by online status and game activity:
    ///
    /// - [`OnlinePlayingGame`](RelationshipGroupType::OnlinePlayingGame) — online
    ///   and currently playing this game.
    /// - [`OnlineElsewhere`](RelationshipGroupType::OnlineElsewhere) — online but
    ///   not playing this game; users who have played it before sort to the top.
    /// - [`Offline`](RelationshipGroupType::Offline) — offline.
    ///
    /// Pair with
    /// [`on_relationship_groups_updated`](Self::on_relationship_groups_updated)
    /// to keep a friends list fresh.
    pub fn relationships_by_group(&self, group_type: RelationshipGroupType) -> Vec<Relationship> {
        // SAFETY: read-only span getter. Ownership of the array and of every
        // handle in it transfers to us, exactly as in `relationships`.
        unsafe {
            span::out(
                |out| {
                    sys::Discord_Client_GetRelationshipsByGroup(
                        self.raw_ptr(),
                        group_type.into_raw(),
                        out,
                    )
                },
                |raw| Relationship::from_raw(raw),
            )
        }
    }

    /// The relationship between the current user and `user_id`.
    ///
    /// Always returns a handle. When there is no relationship — including after
    /// one was deleted — its type fields are
    /// [`RelationshipType::None`](crate::enums::RelationshipType::None).
    pub fn relationship(&self, user_id: u64) -> Relationship {
        let mut raw = MaybeUninit::<sys::Discord_RelationshipHandle>::uninit();
        // SAFETY: read-only getter that always initialises the out-parameter and
        // transfers ownership of the resulting handle to us.
        unsafe {
            sys::Discord_Client_GetRelationshipHandle(self.raw_ptr(), user_id, raw.as_mut_ptr());
            Relationship::from_raw(raw.assume_init())
        }
    }

    /// Fuzzy-search the current user's friends by username and display name.
    ///
    /// Matching uses Levenshtein distance, so near misses still hit. This
    /// searches existing friends only; it is not a directory lookup for
    /// arbitrary Discord users.
    pub fn search_friends_by_username(&self, search: &str) -> Vec<User> {
        // SAFETY: the search string is copied during the call. The returned span
        // is owned by us: every handle is adopted by a `User` and `span::out`
        // frees the backing array.
        unsafe {
            span::out(
                |out| {
                    sys::Discord_Client_SearchFriendsByUsername(
                        self.raw_ptr(),
                        string::borrow(search),
                        out,
                    )
                },
                |raw| User::from_raw(raw),
            )
        }
    }

    // ---- Users ----

    /// The user with the given id, if the SDK already knows about them.
    ///
    /// Returns `None` rather than fetching from Discord's API. Users are
    /// generally available for every relationship and for the author of any
    /// message received.
    pub fn user(&self, user_id: u64) -> Option<User> {
        let mut raw = MaybeUninit::<sys::Discord_UserHandle>::uninit();
        // SAFETY: read-only getter. It only writes the out-parameter when it
        // returns true, so `assume_init` is confined to that branch; the handle
        // it writes is owned by us and dropped by `User`.
        unsafe {
            if sys::Discord_Client_GetUser(self.raw_ptr(), user_id, raw.as_mut_ptr()) {
                Some(User::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The user currently logged in to the Discord desktop application.
    ///
    /// Only available when the Discord app is running on the player's computer
    /// *and* the SDK has established a connection to it; the callback receives
    /// `Ok(None)` when it has not.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn discord_client_connected_user<F>(&mut self, application_id: u64, callback: F)
    where
        F: FnOnce(Result<Option<User>>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            user: *mut sys::Discord_UserHandle,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<Option<User>>) + 'static,
        {
            // SAFETY: `user` is null when no Discord app user is available.
            // Otherwise it points at a handle whose ownership transfers to this
            // callback — the official C++ wrapper adopts it the same way — so it
            // is read out by value and released by the `User` wrapper. The
            // pointer itself belongs to the SDK and is not freed here.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    // Claimed before the result is inspected: the SDK transfers this
                    // payload regardless of outcome, so taking it only on success
                    // would leak it on every failed request.
                    let user = if user.is_null() {
                        None
                    } else {
                        Some(User::from_raw(*user))
                    };
                    let outcome = to_result(result).map(|()| user);
                    f(outcome)
                })
            }
        }
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_GetDiscordClientConnectedUser(
                self.as_raw_mut(),
                application_id,
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Set whether the current user appears online, idle, invisible, or in "do
    /// not disturb" on Discord.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn set_online_status<F>(&mut self, status: StatusType, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the boxed closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_SetOnlineStatus(
                self.as_raw_mut(),
                status.into_raw(),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Events ----

    /// Be notified when a relationship is established or changes type.
    ///
    /// Fires when the user sends or accepts a friend request, or blocks someone.
    ///
    /// The closure receives the other user's id and whether the change was to
    /// the **Discord** relationship (`true`) or the **game** relationship
    /// (`false`). Read the new state with [`relationship`](Self::relationship).
    pub fn on_relationship_created<F>(&mut self, callback: F)
    where
        F: FnMut(u64, bool) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetRelationshipCreatedCallback(
                self.as_raw_mut(),
                Some(relationship_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a relationship is removed.
    ///
    /// Fires when a friend request is rejected or a friend is removed. Once it
    /// has fired, [`relationship`](Self::relationship) reports
    /// [`RelationshipType::None`](crate::enums::RelationshipType::None) for that
    /// user.
    ///
    /// The closure receives the other user's id and whether the removal was of
    /// the **Discord** relationship (`true`) or the **game** relationship
    /// (`false`).
    pub fn on_relationship_deleted<F>(&mut self, callback: F)
    where
        F: FnMut(u64, bool) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetRelationshipDeletedCallback(
                self.as_raw_mut(),
                Some(relationship_event_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified whenever any user in the friends list changes.
    ///
    /// This is the callback to drive a friends list from: rebuild it with
    /// [`relationships_by_group`](Self::relationships_by_group) each time it
    /// fires. The closure receives the id of the user that changed.
    pub fn on_relationship_groups_updated<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetRelationshipGroupsUpdatedCallback(
                self.as_raw_mut(),
                Some(user_id_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified whenever *any* user this session knows about changes.
    ///
    /// Not limited to the current user: it fires when a friend changes their
    /// name or avatar, comes online, goes offline, or starts playing this game.
    /// The closure receives the id of the user that changed; fetch the new state
    /// with [`user`](Self::user).
    pub fn on_user_updated<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetUserUpdatedCallback(
                self.as_raw_mut(),
                Some(user_id_tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }
}
