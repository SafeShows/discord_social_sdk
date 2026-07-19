//! Channels, guilds, and the link between a lobby and a Discord channel.
//!
//! Every message on Discord is sent in a channel. [`Channel`] is the handle for
//! one, obtained from `Client::channel`.
//!
//! The remaining types in this module exist for *channel linking*: a [`Lobby`]
//! can be attached to a text channel in a Discord guild so that messages sent in
//! one appear in the other. Building that flow means listing the user's guilds
//! ([`GuildMinimal`]) and the channels inside them ([`GuildChannel`]), then
//! inspecting the link from either side — [`LinkedLobby`] describes the lobby a
//! channel points at, [`LinkedChannel`] the channel a lobby points at.
//!
//! [`Lobby`]: crate::lobby::Lobby

use std::mem::MaybeUninit;

use discord_social_sdk_sys as sys;

use crate::enums::ChannelType;
use crate::handle::handle;
use crate::{span, string};

handle! {
    /// A Discord channel.
    ///
    /// `Message::channel_id` carries the ID of the channel a message was sent
    /// in, and `Client::channel` turns that ID into one of these.
    ///
    /// # Staleness
    ///
    /// A handle references both the underlying data and the SDK instance.
    /// Updates to the data are visible through existing handles without
    /// re-creating them. If the SDK instance is destroyed while a handle is
    /// still alive, every accessor returns a default value instead — an empty
    /// string for string accessors, for example.
    Channel(sys::Discord_ChannelHandle) {
        drop: sys::Discord_ChannelHandle_Drop,
        clone: sys::Discord_ChannelHandle_Clone,
    }
}

impl Channel {
    /// The ID of the channel.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_ChannelHandle_Id(self.raw_ptr()) }
    }

    /// The name of the channel.
    ///
    /// Generally only channels in guilds have names, though Discord may generate
    /// a display name for some others.
    pub fn name(&self) -> String {
        // SAFETY: the getter initialises the out-parameter and transfers
        // ownership of the buffer to us.
        unsafe { string::out(|out| sys::Discord_ChannelHandle_Name(self.raw_ptr(), out)) }
    }

    /// For DMs and group DMs, the user IDs of the channel's members.
    ///
    /// Empty for every other channel type.
    pub fn recipients(&self) -> Vec<u64> {
        // SAFETY: the getter initialises the span and transfers ownership of the
        // backing array; the elements are plain integers needing no wrapping.
        unsafe {
            span::out(
                |out| sys::Discord_ChannelHandle_Recipients(self.raw_ptr(), out),
                |id| id,
            )
        }
    }

    /// The kind of channel this is.
    pub fn channel_type(&self) -> ChannelType {
        // SAFETY: read-only call on a live handle.
        ChannelType::from_raw(unsafe { sys::Discord_ChannelHandle_Type(self.raw_ptr()) })
    }
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("id", &self.id())
            .field("name", &self.name())
            .field("channel_type", &self.channel_type())
            .finish()
    }
}

handle! {
    /// A guild (a Discord server) the current user belongs to, containing
    /// channels that may be linkable to a lobby.
    ///
    /// The SDK hands these back from `Client::user_guilds`; there is no C
    /// constructor, so instances always originate from the SDK.
    GuildMinimal(sys::Discord_GuildMinimal) {
        drop: sys::Discord_GuildMinimal_Drop,
        clone: sys::Discord_GuildMinimal_Clone,
    }
}

impl GuildMinimal {
    /// The ID of the guild.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_GuildMinimal_Id(self.raw_ptr()) }
    }

    /// Set the ID of the guild.
    pub fn set_id(&mut self, value: u64) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_GuildMinimal_SetId(self.as_raw_mut(), value) }
    }

    /// The name of the guild.
    pub fn name(&self) -> String {
        // SAFETY: the getter initialises the out-parameter and transfers
        // ownership of the buffer to us.
        unsafe { string::out(|out| sys::Discord_GuildMinimal_Name(self.raw_ptr(), out)) }
    }

    /// Set the name of the guild.
    ///
    /// The SDK copies the string during the call, so `value` need not outlive it.
    pub fn set_name(&mut self, value: &str) {
        // SAFETY: `string::borrow` is valid for the duration of the call, and
        // the setter copies its input.
        unsafe { sys::Discord_GuildMinimal_SetName(self.as_raw_mut(), string::borrow(value)) }
    }
}

impl std::fmt::Debug for GuildMinimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuildMinimal")
            .field("id", &self.id())
            .field("name", &self.name())
            .finish()
    }
}

handle! {
    /// A channel in a guild the current user belongs to, which may be linkable
    /// to a lobby.
    ///
    /// Returned by `Client::guild_channels`; there is no C constructor, so
    /// instances always originate from the SDK.
    GuildChannel(sys::Discord_GuildChannel) {
        drop: sys::Discord_GuildChannel_Drop,
        clone: sys::Discord_GuildChannel_Clone,
    }
}

impl GuildChannel {
    /// The ID of the channel.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_GuildChannel_Id(self.raw_ptr()) }
    }

    /// Set the ID of the channel.
    pub fn set_id(&mut self, value: u64) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_GuildChannel_SetId(self.as_raw_mut(), value) }
    }

    /// The name of the channel.
    pub fn name(&self) -> String {
        // SAFETY: the getter initialises the out-parameter and transfers
        // ownership of the buffer to us.
        unsafe { string::out(|out| sys::Discord_GuildChannel_Name(self.raw_ptr(), out)) }
    }

    /// Set the name of the channel.
    ///
    /// The SDK copies the string during the call, so `value` need not outlive it.
    pub fn set_name(&mut self, value: &str) {
        // SAFETY: `string::borrow` is valid for the duration of the call, and
        // the setter copies its input.
        unsafe { sys::Discord_GuildChannel_SetName(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The kind of channel this is.
    pub fn channel_type(&self) -> ChannelType {
        // SAFETY: read-only call on a live handle.
        ChannelType::from_raw(unsafe { sys::Discord_GuildChannel_Type(self.raw_ptr()) })
    }

    /// Set the kind of channel this is.
    pub fn set_channel_type(&mut self, value: ChannelType) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_GuildChannel_SetType(self.as_raw_mut(), value.into_raw()) }
    }

    /// The position of the channel in the guild's channel list.
    pub fn position(&self) -> i32 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_GuildChannel_Position(self.raw_ptr()) }
    }

    /// Set the position of the channel in the guild's channel list.
    pub fn set_position(&mut self, value: i32) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_GuildChannel_SetPosition(self.as_raw_mut(), value) }
    }

    /// The ID of the parent category channel, if any.
    pub fn parent_id(&self) -> Option<u64> {
        let mut raw = MaybeUninit::<u64>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns true.
        unsafe {
            if sys::Discord_GuildChannel_ParentId(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(raw.assume_init())
            } else {
                None
            }
        }
    }

    /// Set the ID of the parent category channel.
    ///
    /// `None` clears the parent.
    pub fn set_parent_id(&mut self, mut value: Option<u64>) {
        let ptr = match value.as_mut() {
            Some(v) => v as *mut u64,
            None => std::ptr::null_mut(),
        };
        // SAFETY: `ptr` is either null (meaning "clear") or points to a live
        // local that outlives the call.
        unsafe { sys::Discord_GuildChannel_SetParentId(self.as_raw_mut(), ptr) }
    }

    /// Whether the current user is able to link this channel to a lobby.
    ///
    /// This requires all of:
    ///
    /// - the channel is a guild text channel,
    /// - the channel is not marked NSFW,
    /// - the channel is not already linked to a different lobby,
    /// - the user holds the Manage Channels, View Channel and Send Messages
    ///   permissions in the channel.
    pub fn is_linkable(&self) -> bool {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_GuildChannel_IsLinkable(self.raw_ptr()) }
    }

    /// Set whether the current user is able to link this channel to a lobby.
    pub fn set_is_linkable(&mut self, value: bool) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_GuildChannel_SetIsLinkable(self.as_raw_mut(), value) }
    }

    /// Whether the channel is "fully public": every member of the guild can view
    /// it and send messages in it.
    ///
    /// Discord permits lobbies to be linked to private channels too, which
    /// enables things like a private admin chat. But there is no permission
    /// synchronisation between the game and Discord, so it is the game's
    /// responsibility to restrict access to the lobby: *every* member of the
    /// lobby can read and write the linked channel whether or not they would
    /// have permission to do so on Discord.
    ///
    /// Rather than take that on, a game can use this flag to offer only fully
    /// public channels for linking, or to show a clear warning before linking a
    /// private one.
    pub fn is_viewable_and_writeable_by_all_members(&self) -> bool {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_GuildChannel_IsViewableAndWriteableByAllMembers(self.raw_ptr()) }
    }

    /// Set whether the channel is viewable and writeable by all guild members.
    pub fn set_is_viewable_and_writeable_by_all_members(&mut self, value: bool) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe {
            sys::Discord_GuildChannel_SetIsViewableAndWriteableByAllMembers(
                self.as_raw_mut(),
                value,
            )
        }
    }

    /// Information about the lobby currently linked to this channel, if any.
    ///
    /// Discord enforces that a channel can be linked to at most one lobby.
    pub fn linked_lobby(&self) -> Option<LinkedLobby> {
        let mut raw = MaybeUninit::<sys::Discord_LinkedLobby>::uninit();
        // SAFETY: the out-parameter is only initialised when the call returns
        // true, and ownership of the value then transfers to us.
        unsafe {
            if sys::Discord_GuildChannel_LinkedLobby(self.raw_ptr(), raw.as_mut_ptr()) {
                Some(LinkedLobby::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Set the lobby linked to this channel.
    ///
    /// `None` clears the link. The argument is borrowed rather than consumed:
    /// the SDK copies it during the call, matching the official C++ wrapper,
    /// which keeps ownership of its own value across the same call.
    pub fn set_linked_lobby(&mut self, value: Option<&LinkedLobby>) {
        let ptr = match value {
            Some(v) => v.raw_ptr(),
            None => std::ptr::null_mut(),
        };
        // SAFETY: `ptr` is either null (meaning "clear") or points to a live
        // handle that outlives the call, which only reads through it.
        unsafe { sys::Discord_GuildChannel_SetLinkedLobby(self.as_raw_mut(), ptr) }
    }
}

impl std::fmt::Debug for GuildChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuildChannel")
            .field("id", &self.id())
            .field("name", &self.name())
            .field("channel_type", &self.channel_type())
            .field("position", &self.position())
            .field("parent_id", &self.parent_id())
            .field("is_linkable", &self.is_linkable())
            .field(
                "is_viewable_and_writeable_by_all_members",
                &self.is_viewable_and_writeable_by_all_members(),
            )
            .field("linked_lobby", &self.linked_lobby())
            .finish()
    }
}

handle! {
    /// Information about the lobby a guild channel is linked to.
    ///
    /// Unlike the other types here this one has a constructor, so a link can be
    /// described locally and handed to
    /// [`GuildChannel::set_linked_lobby`].
    LinkedLobby(sys::Discord_LinkedLobby) {
        init: sys::Discord_LinkedLobby_Init,
        drop: sys::Discord_LinkedLobby_Drop,
        clone: sys::Discord_LinkedLobby_Clone,
    }
}

impl LinkedLobby {
    /// The ID of the application that owns the lobby.
    pub fn application_id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LinkedLobby_ApplicationId(self.raw_ptr()) }
    }

    /// Set the ID of the application that owns the lobby.
    pub fn set_application_id(&mut self, value: u64) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_LinkedLobby_SetApplicationId(self.as_raw_mut(), value) }
    }

    /// The ID of the lobby.
    pub fn lobby_id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LinkedLobby_LobbyId(self.raw_ptr()) }
    }

    /// Set the ID of the lobby.
    pub fn set_lobby_id(&mut self, value: u64) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_LinkedLobby_SetLobbyId(self.as_raw_mut(), value) }
    }
}

impl std::fmt::Debug for LinkedLobby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkedLobby")
            .field("application_id", &self.application_id())
            .field("lobby_id", &self.lobby_id())
            .finish()
    }
}

handle! {
    /// Information about the Discord channel a lobby is linked to.
    ///
    /// Returned by `Lobby::linked_channel`; there is no C constructor, so
    /// instances always originate from the SDK.
    LinkedChannel(sys::Discord_LinkedChannel) {
        drop: sys::Discord_LinkedChannel_Drop,
        clone: sys::Discord_LinkedChannel_Clone,
    }
}

impl LinkedChannel {
    /// The ID of the linked channel.
    pub fn id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LinkedChannel_Id(self.raw_ptr()) }
    }

    /// Set the ID of the linked channel.
    pub fn set_id(&mut self, value: u64) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_LinkedChannel_SetId(self.as_raw_mut(), value) }
    }

    /// The name of the linked channel.
    pub fn name(&self) -> String {
        // SAFETY: the getter initialises the out-parameter and transfers
        // ownership of the buffer to us.
        unsafe { string::out(|out| sys::Discord_LinkedChannel_Name(self.raw_ptr(), out)) }
    }

    /// Set the name of the linked channel.
    ///
    /// The SDK copies the string during the call, so `value` need not outlive it.
    pub fn set_name(&mut self, value: &str) {
        // SAFETY: `string::borrow` is valid for the duration of the call, and
        // the setter copies its input.
        unsafe { sys::Discord_LinkedChannel_SetName(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The ID of the guild that owns the linked channel.
    pub fn guild_id(&self) -> u64 {
        // SAFETY: read-only call on a live handle.
        unsafe { sys::Discord_LinkedChannel_GuildId(self.raw_ptr()) }
    }

    /// Set the ID of the guild that owns the linked channel.
    pub fn set_guild_id(&mut self, value: u64) {
        // SAFETY: mutating call on a uniquely borrowed live handle.
        unsafe { sys::Discord_LinkedChannel_SetGuildId(self.as_raw_mut(), value) }
    }
}

impl std::fmt::Debug for LinkedChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkedChannel")
            .field("id", &self.id())
            .field("name", &self.name())
            .field("guild_id", &self.guild_id())
            .finish()
    }
}
