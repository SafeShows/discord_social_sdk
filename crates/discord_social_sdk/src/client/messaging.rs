//! Sending, editing, and observing chat messages.
//!
//! The SDK carries two kinds of chat, both of which arrive as [`Message`]
//! handles:
//!
//! - **User messages** — one-on-one direct messages between the current user and
//!   one recipient, sent with [`Client::send_user_message`]. A DM can only be
//!   sent when the two users are both online in the game and have not blocked
//!   each other, or are friends, or share a Discord server and have DM'd there
//!   before.
//! - **Lobby messages** — messages sent to every member of a lobby with
//!   [`Client::send_lobby_message`]. If the lobby is linked to a Discord
//!   channel, the message is relayed to that channel too.
//!
//! Message content is capped at 2,000 characters and may contain Discord's
//! [message formatting] markup.
//!
//! # History
//!
//! [`Client::message`] reads the SDK's in-memory cache — the 25 most recent
//! messages per channel. To load more, [`Client::fetch_lobby_messages`] and
//! [`Client::fetch_user_messages`] go out to Discord's API and return up to 200
//! messages from the last 72 hours, newest first.
//!
//! # Notifications
//!
//! When the Discord desktop app is running on the same machine, the user hears
//! its notification for a message they can already see in game. Call
//! [`Client::set_showing_chat`] whenever in-game chat is shown or hidden so
//! Discord can suppress the duplicate.
//!
//! [message formatting]: https://discord.com/developers/docs/reference#message-formatting

use super::Client;
use crate::error::{Result, to_result};
use crate::message::{Message, UserMessageSummary};
use crate::{callback, span, string};
use discord_social_sdk_sys as sys;
use std::ffi::c_void;
use std::mem::MaybeUninit;

/// Build a `Discord_Properties` borrowing Rust memory and run `f` with it.
///
/// `Discord_Properties` is two parallel arrays of `Discord_String`. The SDK
/// copies both the arrays and the strings they point at while the call runs —
/// the official C++ wrapper allocates a temporary copy and frees it the instant
/// the call returns — so borrowing the caller's `&str`s is sound as long as the
/// backing [`Vec`]s outlive the call. Keeping them in scope here is what
/// guarantees that.
///
/// The result must never be passed to `Discord_FreeProperties`: nothing in it
/// was allocated by the SDK.
fn with_properties<T, F>(metadata: &[(&str, &str)], f: F) -> T
where
    F: FnOnce(sys::Discord_Properties) -> T,
{
    let mut keys: Vec<sys::Discord_String> =
        metadata.iter().map(|(k, _)| string::borrow(k)).collect();
    let mut values: Vec<sys::Discord_String> =
        metadata.iter().map(|(_, v)| string::borrow(v)).collect();
    let properties = sys::Discord_Properties {
        size: keys.len(),
        keys: keys.as_mut_ptr(),
        values: values.as_mut_ptr(),
    };
    // `keys` and `values` are still owned by this frame, so the pointers stay
    // valid for the whole of `f`.
    f(properties)
}

/// Trampoline for `Discord_Client_SendUserMessageCallback`.
///
/// Shared by every send site; on success it reports the new message's id.
unsafe extern "C" fn send_tramp<F>(
    result: *mut sys::Discord_ClientResult,
    message_id: u64,
    userdata: *mut c_void,
) where
    F: FnOnce(Result<u64>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result).map(|()| message_id))) }
}

/// Trampoline for callbacks that report only success or failure.
unsafe extern "C" fn result_tramp<F>(result: *mut sys::Discord_ClientResult, userdata: *mut c_void)
where
    F: FnOnce(Result<()>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result))) }
}

/// Trampoline for the two message-fetching callbacks.
///
/// The `Discord_MessageHandleSpan` is **owned**: the SDK transfers the array and
/// every handle in it, exactly as the official C++ wrapper does when it adopts
/// each element as `DiscordObjectState::Owned` and then frees the array. Each
/// element is therefore moved out rather than cloned, and the array released.
/// Treating the span as borrowed would leak every message in it.
unsafe extern "C" fn messages_tramp<F>(
    result: *mut sys::Discord_ClientResult,
    messages: sys::Discord_MessageHandleSpan,
    userdata: *mut c_void,
) where
    F: FnOnce(Result<Vec<Message>>) + 'static,
{
    // SAFETY: the span is transferred to us — the C++ wrapper adopts every element
    // as owned and then frees the array — so elements are moved out, not cloned.
    unsafe {
        callback::dispatch_once::<F>(userdata, |f| {
            // Claimed before the result is inspected: the SDK transfers the span
            // and every handle in it regardless of outcome, so taking it only on
            // success would leak the array and all its messages on every failure.
            let messages = span::take(messages, |raw| Message::from_raw(raw));
            f(to_result(result).map(|()| messages))
        })
    }
}

impl Client {
    // ---- Sending ----

    /// Send a direct message to `recipient_id`.
    ///
    /// The content is capped at 2,000 characters and may contain Discord's
    /// message formatting markup. On success the callback receives the new
    /// message's id.
    ///
    /// A DM can only be delivered when one of the following holds:
    ///
    /// - both users are online in the game and have not blocked each other,
    /// - the users are friends, or
    /// - the users share a Discord server and have DM'd each other on Discord.
    ///
    /// Only send on behalf of a user in response to a user action.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn send_user_message<F>(&mut self, recipient_id: u64, content: &str, callback: F)
    where
        F: FnOnce(Result<u64>) + 'static,
    {
        // SAFETY: the content string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_SendUserMessage(
                self.as_raw_mut(),
                recipient_id,
                string::borrow(content),
                Some(send_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Send a direct message with developer-defined metadata attached.
    ///
    /// `metadata` is plain string key/value pairs travelling alongside the
    /// message — carrying the in-game name of the character who spoke, for
    /// example. It is readable again via [`Message::metadata`].
    ///
    /// Otherwise identical to [`send_user_message`](Self::send_user_message).
    pub fn send_user_message_with_metadata<F>(
        &mut self,
        recipient_id: u64,
        content: &str,
        metadata: &[(&str, &str)],
        callback: F,
    ) where
        F: FnOnce(Result<u64>) + 'static,
    {
        let ptr = self.as_raw_mut();
        with_properties(metadata, |properties| {
            // SAFETY: the content string and the properties both borrow Rust
            // memory that outlives this call, and the SDK copies both before
            // returning.
            unsafe {
                sys::Discord_Client_SendUserMessageWithMetadata(
                    ptr,
                    recipient_id,
                    string::borrow(content),
                    properties,
                    Some(send_tramp::<F>),
                    callback::free_fn::<Option<F>>(),
                    callback::once_userdata(callback),
                )
            }
        })
    }

    /// Send a message to every member of a lobby.
    ///
    /// The content is capped at 2,000 characters and may contain Discord's
    /// message formatting markup. If the lobby is linked to a Discord channel
    /// the message is also posted there. On success the callback receives the
    /// new message's id.
    pub fn send_lobby_message<F>(&mut self, lobby_id: u64, content: &str, callback: F)
    where
        F: FnOnce(Result<u64>) + 'static,
    {
        // SAFETY: the content string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_SendLobbyMessage(
                self.as_raw_mut(),
                lobby_id,
                string::borrow(content),
                Some(send_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Send a lobby message with developer-defined metadata attached.
    ///
    /// `metadata` is plain string key/value pairs, readable again via
    /// [`Message::metadata`].
    pub fn send_lobby_message_with_metadata<F>(
        &mut self,
        lobby_id: u64,
        content: &str,
        metadata: &[(&str, &str)],
        callback: F,
    ) where
        F: FnOnce(Result<u64>) + 'static,
    {
        let ptr = self.as_raw_mut();
        with_properties(metadata, |properties| {
            // SAFETY: the content string and the properties both borrow Rust
            // memory that outlives this call, and the SDK copies both before
            // returning.
            unsafe {
                sys::Discord_Client_SendLobbyMessageWithMetadata(
                    ptr,
                    lobby_id,
                    string::borrow(content),
                    properties,
                    Some(send_tramp::<F>),
                    callback::free_fn::<Option<F>>(),
                    callback::once_userdata(callback),
                )
            }
        })
    }

    // ---- Editing and deleting ----

    /// Edit a direct message the current user sent to `recipient_id`.
    ///
    /// The same restrictions apply as when sending — see
    /// [`send_user_message`](Self::send_user_message).
    pub fn edit_user_message<F>(
        &mut self,
        recipient_id: u64,
        message_id: u64,
        content: &str,
        callback: F,
    ) where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the content string is copied by the SDK during the call.
        unsafe {
            sys::Discord_Client_EditUserMessage(
                self.as_raw_mut(),
                recipient_id,
                message_id,
                string::borrow(content),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Delete a direct message the current user sent to `recipient_id`.
    pub fn delete_user_message<F>(&mut self, recipient_id: u64, message_id: u64, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_DeleteUserMessage(
                self.as_raw_mut(),
                recipient_id,
                message_id,
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Reading ----

    /// Look up a cached message by id.
    ///
    /// The SDK keeps the 25 most recent messages per channel in memory. Anything
    /// older, or sent before the SDK started, yields `None`.
    pub fn message(&self, message_id: u64) -> Option<Message> {
        let mut raw = MaybeUninit::<sys::Discord_MessageHandle>::uninit();
        // SAFETY: read-only call; the handle is initialised and transferred to
        // us only when the call returns `true`.
        unsafe {
            if sys::Discord_Client_GetMessageHandle(self.raw_ptr(), message_id, raw.as_mut_ptr()) {
                Some(Message::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Fetch recent messages from a lobby.
    ///
    /// Returns at most 200 messages, none older than 72 hours, in reverse
    /// chronological order (newest first). The current user must be a member of
    /// the lobby.
    ///
    /// Unlike [`message`](Self::message) this always makes an HTTP request to
    /// Discord's API rather than reading the local cache.
    ///
    /// `limit` caps the number of messages returned.
    pub fn fetch_lobby_messages<F>(&mut self, lobby_id: u64, limit: i32, callback: F)
    where
        F: FnOnce(Result<Vec<Message>>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetLobbyMessagesWithLimit(
                self.as_raw_mut(),
                lobby_id,
                limit,
                Some(messages_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Fetch recent messages from the DM conversation with `recipient_id`.
    ///
    /// Returns at most 200 messages, none older than 72 hours, in reverse
    /// chronological order (newest first). The local cache is consulted first
    /// and Discord's API is only queried when it does not hold enough messages.
    ///
    /// A `limit` greater than zero caps the number of messages returned; zero or
    /// negative means the default of 200 messages and 72 hours. Intended for
    /// loading history when the user opens a conversation.
    ///
    /// If either user has never played the game there is no channel between
    /// them, and the call fails with an HTTP 404
    /// ([`ErrorKind::Http`](crate::ErrorKind::Http)).
    pub fn fetch_user_messages<F>(&mut self, recipient_id: u64, limit: i32, callback: F)
    where
        F: FnOnce(Result<Vec<Message>>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetUserMessagesWithLimit(
                self.as_raw_mut(),
                recipient_id,
                limit,
                Some(messages_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Fetch a summary of every DM conversation the current user has.
    ///
    /// Each [`UserMessageSummary`] names the other user and the id of the most
    /// recent message in that conversation; the messages themselves are loaded
    /// with [`fetch_user_messages`](Self::fetch_user_messages).
    pub fn fetch_user_message_summaries<F>(&mut self, callback: F)
    where
        F: FnOnce(Result<Vec<UserMessageSummary>>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            result: *mut sys::Discord_ClientResult,
            summaries: sys::Discord_UserMessageSummarySpan,
            userdata: *mut c_void,
        ) where
            F: FnOnce(Result<Vec<UserMessageSummary>>) + 'static,
        {
            // SAFETY: the span is owned — the C++ wrapper adopts each element and
            // frees the array — so summaries are moved out rather than cloned.
            unsafe {
                callback::dispatch_once::<F>(userdata, |f| {
                    // Claimed before the result is inspected: the SDK transfers this
                    // payload regardless of outcome, so taking it only on success
                    // would leak it on every failed request.
                    let summaries = span::take(summaries, |raw| UserMessageSummary::from_raw(raw));
                    f(to_result(result).map(|()| summaries))
                })
            }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetUserMessageSummaries(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Handing off to the Discord client ----

    /// Whether a message can be viewed in the Discord client.
    ///
    /// Not all chat is replicated to Discord: lobby chat and some DMs are
    /// ephemeral and never persisted, so they cannot be opened. Check this
    /// before offering the user a "view in Discord" affordance.
    pub fn can_open_message_in_discord(&self, message_id: u64) -> bool {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_Client_CanOpenMessageInDiscord(self.raw_ptr(), message_id) }
    }

    /// Open a message in the Discord client.
    ///
    /// Useful when a message carries content the game cannot render — wire this
    /// to the click handler of whatever prompt offers to show it in Discord.
    ///
    /// `on_merge_required` fires when the user is on a provisional account that
    /// must be merged into a real Discord account before the message can be
    /// opened; drive the merge flow from there. It is separate from `callback`,
    /// which reports whether the open itself succeeded.
    pub fn open_message_in_discord<M, F>(
        &mut self,
        message_id: u64,
        on_merge_required: M,
        callback: F,
    ) where
        M: FnMut() + 'static,
        F: FnOnce(Result<()>) + 'static,
    {
        unsafe extern "C" fn merge_tramp<M: FnMut() + 'static>(userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `M` installed below.
            unsafe { callback::dispatch_mut::<M>(userdata, |f| f()) }
        }
        // SAFETY: the SDK owns both boxed closures and frees each through the
        // `free_fn` paired with it.
        unsafe {
            sys::Discord_Client_OpenMessageInDiscord(
                self.as_raw_mut(),
                message_id,
                Some(merge_tramp::<M>),
                callback::free_fn::<M>(),
                callback::persistent_userdata(on_merge_required),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    // ---- Events ----

    /// Be notified when a message is received, in a lobby or a DM.
    ///
    /// The closure receives the message id; [`message`](Self::message) turns
    /// that into a [`Message`], and [`Message::channel`] identifies where it was
    /// sent.
    ///
    /// Pair this with [`set_showing_chat`](Self::set_showing_chat) so a user
    /// running the Discord desktop app is not notified twice for a message they
    /// can already see in game.
    pub fn on_message_created<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(u64) + 'static>(message_id: u64, userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(message_id)) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetMessageCreatedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a message is deleted.
    ///
    /// The closure receives the message id and the id of the channel it was in.
    /// Messages sent from a connected user's Discord client — and some sent from
    /// in game — can be deleted there, so handling this keeps the in-game view
    /// in step with Discord.
    pub fn on_message_deleted<F>(&mut self, callback: F)
    where
        F: FnMut(u64, u64) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(u64, u64) + 'static>(
            message_id: u64,
            channel_id: u64,
            userdata: *mut c_void,
        ) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(message_id, channel_id)) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetMessageDeletedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Be notified when a message is edited.
    ///
    /// The closure receives the message id; re-read the message with
    /// [`message`](Self::message) to see the new content. As with deletion,
    /// edits can originate from the user's Discord client.
    pub fn on_message_updated<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(u64) + 'static>(message_id: u64, userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(message_id)) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetMessageUpdatedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Tell Discord whether the game is currently showing chat.
    ///
    /// While this is `true`, the Discord desktop app suppresses its own
    /// notifications for messages the user can already see in game. Set it back
    /// to `false` as soon as chat is hidden — or the game loses focus — so the
    /// user starts hearing Discord's notifications again.
    pub fn set_showing_chat(&mut self, showing_chat: bool) {
        // SAFETY: a plain by-value write to an initialised handle.
        unsafe { sys::Discord_Client_SetShowingChat(self.as_raw_mut(), showing_chat) }
    }
}
