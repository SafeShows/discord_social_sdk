//! Chat messages, their non-text attachments, and DM conversation summaries.
//!
//! The SDK supports two kinds of chat: one-on-one conversations between two
//! users, and chat inside a lobby. Both arrive as [`Message`] handles, and
//! [`Message::channel`] tells you which context a given message came from.
//!
//! # Syncing with Discord
//!
//! Messages sent through the SDK sometimes also appear in the Discord client:
//! for one-on-one chat when at least one participant is a full Discord user, and
//! for lobby chat when the lobby is linked to a Discord channel. The sender must
//! also not be banned on the Discord side.
//!
//! # Legal disclosures
//!
//! The first time a user sends an in-game message that will also show up in
//! Discord, the SDK injects a synthetic message explaining that to the user.
//! Those are identified by [`Message::disclosure_type`] returning `Some`.
//! Discord encourages games to restyle, reword or translate these rather than
//! rendering them verbatim — or to skip rendering them entirely.
//!
//! # History
//!
//! The SDK keeps the 25 most recent messages per channel in memory and has no
//! access to anything sent before it connected. An existing [`Message`] keeps
//! working after the SDK has discarded the message for being too old; you just
//! cannot obtain a new handle for it.
//!
//! # Unrenderable content
//!
//! Messages can carry images, videos, embeds, polls and so on that a game is not
//! expected to render. [`Message::additional_content`] describes that content so
//! the game can show a small notice pointing the user to Discord.
//!
//! See the [message resource documentation] for more on Discord's message model.
//!
//! [message resource documentation]: https://discord.com/developers/docs/resources/message

use crate::channel::Channel;
use crate::enums::{AdditionalContentType, DisclosureType};
use crate::handle::handle;
use crate::lobby::Lobby;
use crate::string;
use crate::user::User;
use discord_social_sdk_sys as sys;
use std::collections::HashMap;
use std::mem::MaybeUninit;

/// Call a getter of the shape `void G(self, Discord_Properties* out)` and adopt
/// the result as a map.
///
/// The SDK allocates the key and value arrays as one block, and the individual
/// strings belong to that same allocation — so they are copied out rather than
/// freed individually, and the block is released with `Discord_FreeProperties`.
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
    // SAFETY: `f` is contracted to have initialised the out-parameter.
    let props = unsafe { raw.assume_init() };

    let mut map = HashMap::new();
    if !props.keys.is_null() && !props.values.is_null() {
        for i in 0..props.size {
            // SAFETY: both arrays hold `props.size` initialised `Discord_String`s,
            // and `view` copies out of them without taking ownership.
            let (key, value) = unsafe {
                (
                    string::view(*props.keys.add(i)),
                    string::view(*props.values.add(i)),
                )
            };
            map.insert(key, value);
        }
    }
    // SAFETY: `props` was just handed to us by the SDK and is released exactly once.
    unsafe { sys::Discord_FreeProperties(props) };
    map
}

handle! {
    /// Non-text content attached to a message.
    ///
    /// Describes content that likely cannot be rendered in game — images,
    /// videos, embeds, polls and similar — so the game can show a notice that
    /// there is more to see on Discord.
    AdditionalContent(sys::Discord_AdditionalContent) {
        init: sys::Discord_AdditionalContent_Init,
        drop: sys::Discord_AdditionalContent_Drop,
        clone: sys::Discord_AdditionalContent_Clone,
    }
}

impl AdditionalContent {
    /// Render an [`AdditionalContentType`] as a human-readable string.
    pub fn type_to_string(content_type: AdditionalContentType) -> String {
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of the buffer to us.
        unsafe {
            string::out(|out| {
                sys::Discord_AdditionalContent_TypeToString(content_type.into_raw(), out)
            })
        }
    }

    /// The kind of additional content in the message.
    ///
    /// Named `content_type` because `type` is a Rust keyword.
    pub fn content_type(&self) -> AdditionalContentType {
        // SAFETY: read-only call on an initialised handle.
        let raw = unsafe { sys::Discord_AdditionalContent_Type(self.raw_ptr()) };
        AdditionalContentType::from_raw(raw)
    }

    /// Set the kind of additional content.
    pub fn set_content_type(&mut self, value: AdditionalContentType) {
        // SAFETY: setter on an initialised handle we own.
        unsafe { sys::Discord_AdditionalContent_SetType(self.as_raw_mut(), value.into_raw()) }
    }

    /// The name of the poll or thread, when the additional content is one of those.
    pub fn title(&self) -> Option<String> {
        // SAFETY: read-only call; the out-parameter is only initialised — and
        // then owned by us — when the call returns `true`.
        unsafe { string::out_opt(|out| sys::Discord_AdditionalContent_Title(self.raw_ptr(), out)) }
    }

    /// Set the title, or clear it with `None`.
    pub fn set_title(&mut self, value: Option<&str>) {
        let ptr = self.as_raw_mut();
        string::with_opt(value, |s| {
            // SAFETY: the SDK copies the string during the call, so borrowing is sound.
            unsafe { sys::Discord_AdditionalContent_SetTitle(ptr, s) }
        })
    }

    /// How many pieces of additional content the message carries.
    ///
    /// Useful for rendering a notice such as "2 additional images".
    pub fn count(&self) -> u8 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_AdditionalContent_Count(self.raw_ptr()) }
    }

    /// Set the number of pieces of additional content.
    pub fn set_count(&mut self, value: u8) {
        // SAFETY: setter on an initialised handle we own.
        unsafe { sys::Discord_AdditionalContent_SetCount(self.as_raw_mut(), value) }
    }
}

impl PartialEq for AdditionalContent {
    /// Compares each field of the additional content for equality.
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: both handles are initialised; `Equals` only reads them.
        unsafe { sys::Discord_AdditionalContent_Equals(self.raw_ptr(), other.as_raw()) }
    }
}

impl std::fmt::Debug for AdditionalContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdditionalContent")
            .field("content_type", &self.content_type())
            .field("title", &self.title())
            .field("count", &self.count())
            .finish()
    }
}

handle! {
    /// A single message received by the SDK.
    ///
    /// Handles hold a reference to both the underlying data and the SDK
    /// instance, so changes to the message are visible through an existing
    /// handle without re-creating it. If the SDK instance is destroyed while a
    /// handle is still alive, every accessor returns a default value (an empty
    /// string, zero, `None`) rather than failing.
    ///
    /// Note: while the SDK lets you send messages on a user's behalf, you must
    /// only do so in response to a user action — never automatically.
    ///
    /// Messages are only ever handed back by the SDK, so there is no constructor.
    Message(sys::Discord_MessageHandle) {
        drop: sys::Discord_MessageHandle_Drop,
        clone: sys::Discord_MessageHandle_Clone,
    }
}

impl Message {
    /// The ID of this message.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_MessageHandle_Id(self.raw_ptr()) }
    }

    /// Information about non-text content in this message, if it has any.
    ///
    /// Images, videos, embeds, polls and so on.
    pub fn additional_content(&self) -> Option<AdditionalContent> {
        let mut raw = MaybeUninit::<sys::Discord_AdditionalContent>::uninit();
        // SAFETY: read-only call; the out-parameter is initialised and
        // ownership transferred only when the call returns `true`.
        unsafe {
            if sys::Discord_MessageHandle_AdditionalContent(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(AdditionalContent::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The application ID associated with this message, if any.
    ///
    /// Identifies whether the message came from another child application in
    /// your catalogue. Parent/child applications are in limited access, so
    /// [`sent_from_game`](Self::sent_from_game) is the field to rely on in the
    /// common case.
    pub fn application_id(&self) -> Option<u64> {
        let mut raw = MaybeUninit::<u64>::uninit();
        // SAFETY: read-only call; the out-parameter is only initialised when
        // the call returns `true`.
        unsafe {
            if sys::Discord_MessageHandle_ApplicationId(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(raw.assume_init())
            } else {
                None
            }
        }
    }

    /// The author of this message.
    pub fn author(&self) -> Option<User> {
        let mut raw = MaybeUninit::<sys::Discord_UserHandle>::uninit();
        // SAFETY: read-only call; the handle is initialised and transferred to
        // us only when the call returns `true`.
        unsafe {
            if sys::Discord_MessageHandle_Author(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(User::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The user ID of whoever sent this message.
    pub fn author_id(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_MessageHandle_AuthorId(self.raw_ptr()) }
    }

    /// The channel this message was sent in.
    ///
    /// Combined with the channel's type this identifies the chat context; the
    /// SDK only receives messages in DM, ephemeral DM and lobby channels.
    pub fn channel(&self) -> Option<Channel> {
        let mut raw = MaybeUninit::<sys::Discord_ChannelHandle>::uninit();
        // SAFETY: read-only call; the handle is initialised and transferred to
        // us only when the call returns `true`.
        unsafe {
            if sys::Discord_MessageHandle_Channel(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(Channel::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The ID of the channel this message was sent in.
    pub fn channel_id(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_MessageHandle_ChannelId(self.raw_ptr()) }
    }

    /// The content of this message.
    ///
    /// May be empty: a message sent from Discord can consist only of image
    /// attachments. Some markup — emoji and mentions — is replaced with a more
    /// human-readable form such as `@username` or `:emoji_name:`. Use
    /// [`raw_content`](Self::raw_content) for the unmodified text.
    pub fn content(&self) -> String {
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of the buffer to us.
        unsafe { string::out(|out| sys::Discord_MessageHandle_Content(self.raw_ptr(), out)) }
    }

    /// The content of this message without markup replacement.
    ///
    /// May be empty for the same reason as [`content`](Self::content).
    pub fn raw_content(&self) -> String {
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of the buffer to us.
        unsafe { string::out(|out| sys::Discord_MessageHandle_RawContent(self.raw_ptr(), out)) }
    }

    /// The disclosure this message is explaining, if it is an auto-generated
    /// disclosure message.
    ///
    /// `Some` marks the synthetic messages the SDK injects to explain
    /// integration behaviour to users, which games are encouraged to restyle.
    pub fn disclosure_type(&self) -> Option<DisclosureType> {
        let mut raw = MaybeUninit::<sys::Discord_DisclosureTypes>::uninit();
        // SAFETY: read-only call; the out-parameter is only initialised when
        // the call returns `true`.
        unsafe {
            if sys::Discord_MessageHandle_DisclosureType(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(DisclosureType::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// When the message was most recently edited, in milliseconds since the
    /// Unix epoch.
    ///
    /// Zero if the message has never been edited.
    pub fn edited_timestamp(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_MessageHandle_EditedTimestamp(self.raw_ptr()) }
    }

    /// When the message was sent, in milliseconds since the Unix epoch.
    pub fn sent_timestamp(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_MessageHandle_SentTimestamp(self.raw_ptr()) }
    }

    /// The lobby this message was sent in, if it was sent in one.
    pub fn lobby(&self) -> Option<Lobby> {
        let mut raw = MaybeUninit::<sys::Discord_LobbyHandle>::uninit();
        // SAFETY: read-only call; the handle is initialised and transferred to
        // us only when the call returns `true`.
        unsafe {
            if sys::Discord_MessageHandle_Lobby(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(Lobby::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Developer-supplied metadata attached to this message.
    ///
    /// Simple string key/value pairs. One use is carrying a character name so
    /// the game can customise how the message renders.
    pub fn metadata(&self) -> HashMap<String, String> {
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of it to us.
        unsafe { properties_out(|out| sys::Discord_MessageHandle_Metadata(self.raw_ptr(), out)) }
    }

    /// Moderation metadata the developer set on this message.
    ///
    /// Simple string key/value pairs. Uses include a flag recording the
    /// message's moderation status, or a rewritten version of the message that
    /// is more appropriate for the game's audience.
    pub fn moderation_metadata(&self) -> HashMap<String, String> {
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of it to us.
        unsafe {
            properties_out(|out| sys::Discord_MessageHandle_ModerationMetadata(self.raw_ptr(), out))
        }
    }

    /// The other participant, if this message was sent in a DM.
    pub fn recipient(&self) -> Option<User> {
        let mut raw = MaybeUninit::<sys::Discord_UserHandle>::uninit();
        // SAFETY: read-only call; the handle is initialised and transferred to
        // us only when the call returns `true`.
        unsafe {
            if sys::Discord_MessageHandle_Recipient(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(User::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// The ID of the other user, when this message was sent in a DM or
    /// ephemeral DM.
    pub fn recipient_id(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_MessageHandle_RecipientId(self.raw_ptr()) }
    }

    /// Whether this message was sent in game rather than from Discord itself.
    ///
    /// With parent/child applications this is true for messages sent from any
    /// child application.
    pub fn sent_from_game(&self) -> bool {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_MessageHandle_SentFromGame(self.raw_ptr()) }
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id())
            .field("author_id", &self.author_id())
            .field("channel_id", &self.channel_id())
            .field("content", &self.content())
            .field("sent_timestamp", &self.sent_timestamp())
            .finish()
    }
}

handle! {
    /// A summary of a DM conversation with a user.
    ///
    /// Identifies the conversation and its most recent message; the messages
    /// themselves are fetched separately.
    UserMessageSummary(sys::Discord_UserMessageSummary) {
        drop: sys::Discord_UserMessageSummary_Drop,
        clone: sys::Discord_UserMessageSummary_Clone,
    }
}

impl UserMessageSummary {
    /// The ID of the last message sent in the DM conversation.
    pub fn last_message_id(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_UserMessageSummary_LastMessageId(self.raw_ptr()) }
    }

    /// The ID of the other user in the DM conversation.
    pub fn user_id(&self) -> u64 {
        // SAFETY: read-only call on an initialised handle.
        unsafe { sys::Discord_UserMessageSummary_UserId(self.raw_ptr()) }
    }
}

impl std::fmt::Debug for UserMessageSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserMessageSummary")
            .field("user_id", &self.user_id())
            .field("last_message_id", &self.last_message_id())
            .finish()
    }
}
