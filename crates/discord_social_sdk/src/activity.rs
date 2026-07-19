//! Rich presence: what a user is doing, as shown on their Discord profile.
//!
//! An [`Activity`] is the whole payload published with
//! `Client::update_rich_presence`. The satellite types fill in its optional
//! parts: [`ActivityAssets`] for artwork, [`ActivityTimestamps`] for elapsed and
//! remaining timers, [`ActivityParty`] for group size, [`ActivitySecrets`] for
//! the join secret that makes a presence joinable, and [`ActivityButton`] for
//! custom link buttons.
//!
//! [`ActivityInvite`] travels the other direction — it is what the SDK hands the
//! game when another user invites the current user to join their game.
//!
//! See the [rich presence overview] for how each field is rendered.
//!
//! [rich presence overview]: https://discord.com/developers/docs/rich-presence/overview

use discord_social_sdk_sys as sys;
use std::mem::MaybeUninit;

use crate::enums::{
    ActivityActionType, ActivityGamePlatforms, ActivityPartyPrivacy, ActivityType,
    StatusDisplayType,
};
use crate::handle::handle;
use crate::{span, string};

/// Call an optional SDK getter of the shape `bool Get(self, T* out)`, where
/// `false` means the field is absent and the out-parameter is left untouched.
///
/// The string-valued equivalent lives in [`crate::string::out_opt`]; this one
/// covers plain values, enums and handles.
///
/// # Safety
///
/// `f` must fully initialise the out-parameter whenever it returns `true`, and
/// must transfer ownership of it when the value is a handle.
unsafe fn opt_out<T, F>(f: F) -> Option<T>
where
    F: FnOnce(*mut T) -> bool,
{
    let mut raw = MaybeUninit::<T>::uninit();
    if f(raw.as_mut_ptr()) {
        Some(unsafe { raw.assume_init() })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// ActivityAssets
// ---------------------------------------------------------------------------

handle! {
    /// Images used to customise how an [`Activity`] is displayed in Discord.
    ///
    /// If nothing is specified, the icon and name of the application are used as
    /// defaults.
    ///
    /// Every image field accepts either the unique identifier of an image
    /// uploaded to the application through the *Rich Presence* page of the
    /// Developer Portal, or an external image URL. So an asset uploaded under the
    /// name `goofy-icon` can be referenced as `"goofy-icon"`, and an image hosted
    /// at `http://my-site.com/goofy.jpg` can be referenced by that URL.
    ///
    /// See [adding custom art assets] for visual examples of what each field does.
    ///
    /// [adding custom art assets]: https://discord.com/developers/docs/rich-presence/overview#adding-custom-art-assets
    ActivityAssets(sys::Discord_ActivityAssets) {
        init: sys::Discord_ActivityAssets_Init,
        drop: sys::Discord_ActivityAssets_Drop,
        clone: sys::Discord_ActivityAssets_Clone,
    }
}

impl ActivityAssets {
    /// The primary image identifier or URL, rendered as a large square icon.
    ///
    /// If specified, must be between 1 and 300 characters.
    pub fn large_image(&self) -> Option<String> {
        // SAFETY: the getter only reads, and hands us ownership of the string it
        // writes when it returns `true`.
        unsafe { string::out_opt(|p| sys::Discord_ActivityAssets_LargeImage(self.raw_ptr(), p)) }
    }

    /// Set the primary image, or clear it with `None`.
    pub fn set_large_image(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string during the call, so borrowing is
        // sound; a null pointer clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_ActivityAssets_SetLargeImage(self.as_raw_mut(), p)
        })
    }

    /// A tooltip shown when the user hovers over the large image.
    ///
    /// If specified, must be between 2 and 128 characters.
    pub fn large_text(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_ActivityAssets_LargeText(self.raw_ptr(), p)) }
    }

    /// Set the large image tooltip, or clear it with `None`.
    pub fn set_large_text(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_ActivityAssets_SetLargeText(self.as_raw_mut(), p)
        })
    }

    /// A URL opened when the user clicks or taps the large image.
    ///
    /// If specified, must be between 1 and 256 characters.
    pub fn large_url(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_ActivityAssets_LargeUrl(self.raw_ptr(), p)) }
    }

    /// Set the large image link, or clear it with `None`.
    pub fn set_large_url(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_ActivityAssets_SetLargeUrl(self.as_raw_mut(), p)
        })
    }

    /// The secondary image, rendered as a small circle over the large image.
    ///
    /// If specified, must be between 1 and 300 characters.
    pub fn small_image(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_ActivityAssets_SmallImage(self.raw_ptr(), p)) }
    }

    /// Set the secondary image, or clear it with `None`.
    pub fn set_small_image(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_ActivityAssets_SetSmallImage(self.as_raw_mut(), p)
        })
    }

    /// A tooltip shown when the user hovers over the small image.
    ///
    /// If specified, must be between 2 and 128 characters.
    pub fn small_text(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_ActivityAssets_SmallText(self.raw_ptr(), p)) }
    }

    /// Set the small image tooltip, or clear it with `None`.
    pub fn set_small_text(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_ActivityAssets_SetSmallText(self.as_raw_mut(), p)
        })
    }

    /// A URL opened when the user clicks or taps the small image.
    ///
    /// If specified, must be between 1 and 256 characters.
    pub fn small_url(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_ActivityAssets_SmallUrl(self.raw_ptr(), p)) }
    }

    /// Set the small image link, or clear it with `None`.
    pub fn set_small_url(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_ActivityAssets_SetSmallUrl(self.as_raw_mut(), p)
        })
    }

    /// The image identifier or URL rendered as a banner on activity invites.
    ///
    /// If specified, must be between 1 and 300 characters.
    pub fn invite_cover_image(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe {
            string::out_opt(|p| sys::Discord_ActivityAssets_InviteCoverImage(self.raw_ptr(), p))
        }
    }

    /// Set the invite cover image, or clear it with `None`.
    pub fn set_invite_cover_image(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_ActivityAssets_SetInviteCoverImage(self.as_raw_mut(), p)
        })
    }
}

impl std::fmt::Debug for ActivityAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityAssets")
            .field("large_image", &self.large_image())
            .field("large_text", &self.large_text())
            .field("large_url", &self.large_url())
            .field("small_image", &self.small_image())
            .field("small_text", &self.small_text())
            .field("small_url", &self.small_url())
            .field("invite_cover_image", &self.invite_cover_image())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ActivityTimestamps
// ---------------------------------------------------------------------------

handle! {
    /// The start and end times of an [`Activity`].
    ///
    /// Used to render either a "time elapsed" count-up timer (by setting
    /// [`start`](ActivityTimestamps::set_start)) or a "time remaining" countdown
    /// (by setting [`end`](ActivityTimestamps::set_end)).
    ActivityTimestamps(sys::Discord_ActivityTimestamps) {
        init: sys::Discord_ActivityTimestamps_Init,
        drop: sys::Discord_ActivityTimestamps_Drop,
        clone: sys::Discord_ActivityTimestamps_Clone,
    }
}

impl ActivityTimestamps {
    /// The time the activity started, in milliseconds since the Unix epoch.
    pub fn start(&self) -> u64 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityTimestamps_Start(self.raw_ptr()) }
    }

    /// Set the start time, in milliseconds since the Unix epoch.
    ///
    /// The SDK converts small-ish values from seconds to milliseconds. When set,
    /// Discord renders a count-up timer showing how long the user has been in
    /// this activity.
    pub fn set_start(&mut self, value: u64) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityTimestamps_SetStart(self.as_raw_mut(), value) }
    }

    /// The time the activity ends, in milliseconds since the Unix epoch.
    pub fn end(&self) -> u64 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityTimestamps_End(self.raw_ptr()) }
    }

    /// Set the end time, in milliseconds since the Unix epoch.
    ///
    /// The SDK converts small-ish values from seconds to milliseconds. When set,
    /// Discord renders a countdown showing how long until the activity ends.
    pub fn set_end(&mut self, value: u64) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityTimestamps_SetEnd(self.as_raw_mut(), value) }
    }
}

impl std::fmt::Debug for ActivityTimestamps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityTimestamps")
            .field("start", &self.start())
            .field("end", &self.end())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ActivityParty
// ---------------------------------------------------------------------------

handle! {
    /// The group of players the current user is playing with.
    ///
    /// Drives the "In a group (2 of 3)" line of a rich presence.
    ActivityParty(sys::Discord_ActivityParty) {
        init: sys::Discord_ActivityParty_Init,
        drop: sys::Discord_ActivityParty_Drop,
        clone: sys::Discord_ActivityParty_Clone,
    }
}

impl ActivityParty {
    /// The id of the party.
    pub fn id(&self) -> String {
        // SAFETY: read-only getter that transfers ownership of the string to us.
        unsafe { string::out(|p| sys::Discord_ActivityParty_Id(self.raw_ptr(), p)) }
    }

    /// Set the id of the party.
    ///
    /// "Party" refers colloquially to a group of players in a shared context —
    /// a lobby id, server id, team id and so on. Every member of the party should
    /// publish the same party id so Discord knows to group them together. Must be
    /// between 2 and 128 characters.
    pub fn set_id(&mut self, value: &str) {
        // SAFETY: the string is passed by value and copied during the call, so
        // the borrow only needs to survive the call itself.
        unsafe { sys::Discord_ActivityParty_SetId(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The number of people currently in the party.
    pub fn current_size(&self) -> i32 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityParty_CurrentSize(self.raw_ptr()) }
    }

    /// Set the number of people currently in the party; must be at least 1.
    pub fn set_current_size(&mut self, value: i32) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityParty_SetCurrentSize(self.as_raw_mut(), value) }
    }

    /// The maximum number of people that can be in the party.
    pub fn max_size(&self) -> i32 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityParty_MaxSize(self.raw_ptr()) }
    }

    /// Set the maximum party size; must be at least 0.
    ///
    /// When 0, Discord does not display a maximum.
    pub fn set_max_size(&mut self, value: i32) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityParty_SetMaxSize(self.as_raw_mut(), value) }
    }

    /// The privacy of the party.
    pub fn privacy(&self) -> ActivityPartyPrivacy {
        // SAFETY: read-only getter on an initialised handle.
        ActivityPartyPrivacy::from_raw(unsafe {
            sys::Discord_ActivityParty_Privacy(self.raw_ptr())
        })
    }

    /// Set the privacy of the party.
    pub fn set_privacy(&mut self, value: ActivityPartyPrivacy) {
        // SAFETY: passing a plain enum value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityParty_SetPrivacy(self.as_raw_mut(), value.into_raw()) }
    }
}

impl std::fmt::Debug for ActivityParty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityParty")
            .field("id", &self.id())
            .field("current_size", &self.current_size())
            .field("max_size", &self.max_size())
            .field("privacy", &self.privacy())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ActivitySecrets
// ---------------------------------------------------------------------------

handle! {
    /// The secret that makes an [`Activity`] joinable.
    ///
    /// Used together with [`ActivityParty`]: without a join secret, Discord has
    /// nothing to hand a user who accepts an invite.
    ActivitySecrets(sys::Discord_ActivitySecrets) {
        init: sys::Discord_ActivitySecrets_Init,
        drop: sys::Discord_ActivitySecrets_Drop,
        clone: sys::Discord_ActivitySecrets_Clone,
    }
}

impl ActivitySecrets {
    /// The join secret.
    pub fn join(&self) -> String {
        // SAFETY: read-only getter that transfers ownership of the string to us.
        unsafe { string::out(|p| sys::Discord_ActivitySecrets_Join(self.raw_ptr(), p)) }
    }

    /// Set the join secret.
    ///
    /// This string is shared with users who are accepted into the party, so the
    /// game knows how to join them — an internal game server id, or a Discord
    /// lobby id or secret, for example. Must be between 2 and 128 characters.
    pub fn set_join(&mut self, value: &str) {
        // SAFETY: the string is passed by value and copied during the call.
        unsafe { sys::Discord_ActivitySecrets_SetJoin(self.as_raw_mut(), string::borrow(value)) }
    }
}

impl std::fmt::Debug for ActivitySecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The secret itself is deliberately not printed.
        f.debug_struct("ActivitySecrets")
            .field("join", &format_args!("<{} chars>", self.join().len()))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ActivityButton
// ---------------------------------------------------------------------------

handle! {
    /// A custom link button rendered on a rich presence.
    ActivityButton(sys::Discord_ActivityButton) {
        init: sys::Discord_ActivityButton_Init,
        drop: sys::Discord_ActivityButton_Drop,
        clone: sys::Discord_ActivityButton_Clone,
    }
}

impl ActivityButton {
    /// Create a button with the given label and target URL.
    pub fn with_label_and_url(label: &str, url: &str) -> Self {
        let mut button = Self::new();
        button.set_label(label);
        button.set_url(url);
        button
    }

    /// The label of the button.
    pub fn label(&self) -> String {
        // SAFETY: read-only getter that transfers ownership of the string to us.
        unsafe { string::out(|p| sys::Discord_ActivityButton_Label(self.raw_ptr(), p)) }
    }

    /// Set the label of the button.
    pub fn set_label(&mut self, value: &str) {
        // SAFETY: the string is passed by value and copied during the call.
        unsafe { sys::Discord_ActivityButton_SetLabel(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The URL the button opens.
    pub fn url(&self) -> String {
        // SAFETY: read-only getter that transfers ownership of the string to us.
        unsafe { string::out(|p| sys::Discord_ActivityButton_Url(self.raw_ptr(), p)) }
    }

    /// Set the URL the button opens.
    pub fn set_url(&mut self, value: &str) {
        // SAFETY: the string is passed by value and copied during the call.
        unsafe { sys::Discord_ActivityButton_SetUrl(self.as_raw_mut(), string::borrow(value)) }
    }
}

impl std::fmt::Debug for ActivityButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityButton")
            .field("label", &self.label())
            .field("url", &self.url())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ActivityInvite
// ---------------------------------------------------------------------------

handle! {
    /// An invitation from one user to another to join their game.
    ///
    /// When a user invites another to join their game on Discord, Discord sends
    /// the recipient a message. The SDK parses those messages automatically, and
    /// this type carries everything needed to later accept the invite.
    ActivityInvite(sys::Discord_ActivityInvite) {
        init: sys::Discord_ActivityInvite_Init,
        drop: sys::Discord_ActivityInvite_Drop,
        clone: sys::Discord_ActivityInvite_Clone,
    }
}

impl ActivityInvite {
    /// The user id of the user who sent the invite.
    pub fn sender_id(&self) -> u64 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SenderId(self.raw_ptr()) }
    }

    /// Set the user id of the sender.
    pub fn set_sender_id(&mut self, value: u64) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SetSenderId(self.as_raw_mut(), value) }
    }

    /// The id of the Discord channel the invite was sent in.
    pub fn channel_id(&self) -> u64 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_ChannelId(self.raw_ptr()) }
    }

    /// Set the id of the channel the invite was sent in.
    pub fn set_channel_id(&mut self, value: u64) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SetChannelId(self.as_raw_mut(), value) }
    }

    /// The id of the Discord message containing the invite.
    pub fn message_id(&self) -> u64 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_MessageId(self.raw_ptr()) }
    }

    /// Set the id of the message containing the invite.
    pub fn set_message_id(&mut self, value: u64) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SetMessageId(self.as_raw_mut(), value) }
    }

    /// The type of invite that was sent.
    pub fn invite_type(&self) -> ActivityActionType {
        // SAFETY: read-only getter on an initialised handle.
        ActivityActionType::from_raw(unsafe { sys::Discord_ActivityInvite_Type(self.raw_ptr()) })
    }

    /// Set the type of invite.
    pub fn set_invite_type(&mut self, value: ActivityActionType) {
        // SAFETY: passing a plain enum value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SetType(self.as_raw_mut(), value.into_raw()) }
    }

    /// The target application of the invite.
    pub fn application_id(&self) -> u64 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_ApplicationId(self.raw_ptr()) }
    }

    /// Set the target application of the invite.
    pub fn set_application_id(&mut self, value: u64) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SetApplicationId(self.as_raw_mut(), value) }
    }

    /// The application id of the parent application, if there is one.
    ///
    /// Only applicable when the application belongs to a publisher's suite of
    /// applications.
    pub fn parent_application_id(&self) -> u64 {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_ParentApplicationId(self.raw_ptr()) }
    }

    /// Set the parent application id.
    pub fn set_parent_application_id(&mut self, value: u64) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SetParentApplicationId(self.as_raw_mut(), value) }
    }

    /// The id of the party the invite was sent for.
    pub fn party_id(&self) -> String {
        // SAFETY: read-only getter that transfers ownership of the string to us.
        unsafe { string::out(|p| sys::Discord_ActivityInvite_PartyId(self.raw_ptr(), p)) }
    }

    /// Set the id of the party the invite is for.
    pub fn set_party_id(&mut self, value: &str) {
        // SAFETY: the string is passed by value and copied during the call.
        unsafe { sys::Discord_ActivityInvite_SetPartyId(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The session id of the user who sent the invite.
    pub fn session_id(&self) -> String {
        // SAFETY: read-only getter that transfers ownership of the string to us.
        unsafe { string::out(|p| sys::Discord_ActivityInvite_SessionId(self.raw_ptr(), p)) }
    }

    /// Set the session id of the sender.
    pub fn set_session_id(&mut self, value: &str) {
        // SAFETY: the string is passed by value and copied during the call.
        unsafe { sys::Discord_ActivityInvite_SetSessionId(self.as_raw_mut(), string::borrow(value)) }
    }

    /// Whether the invite is currently joinable.
    ///
    /// An invite becomes invalid once it is more than six hours old, or once the
    /// sender stops playing the game the invite is for.
    pub fn is_valid(&self) -> bool {
        // SAFETY: read-only getter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_IsValid(self.raw_ptr()) }
    }

    /// Set whether the invite is currently joinable.
    pub fn set_is_valid(&mut self, value: bool) {
        // SAFETY: passing a plain value to a setter on an initialised handle.
        unsafe { sys::Discord_ActivityInvite_SetIsValid(self.as_raw_mut(), value) }
    }
}

impl std::fmt::Debug for ActivityInvite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivityInvite")
            .field("sender_id", &self.sender_id())
            .field("channel_id", &self.channel_id())
            .field("message_id", &self.message_id())
            .field("invite_type", &self.invite_type())
            .field("application_id", &self.application_id())
            .field("parent_application_id", &self.parent_application_id())
            .field("party_id", &self.party_id())
            .field("session_id", &self.session_id())
            .field("is_valid", &self.is_valid())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Activity
// ---------------------------------------------------------------------------

handle! {
    /// One "thing" a user is doing on Discord — their rich presence.
    ///
    /// Rich presence supports several activity types, but the only one really
    /// relevant to this SDK is [`ActivityType::Playing`]. The SDK also only
    /// exposes activities associated with the current game, so a field such as
    /// [`name`](Activity::name) is always the current game's name from the SDK's
    /// point of view.
    ///
    /// # Customisation
    ///
    /// A rendered activity looks roughly like:
    ///
    /// ```text
    /// Playing "game name"
    /// Capture the flag | 2 - 1
    /// In a group (2 of 3)
    /// ```
    ///
    /// - Line 1 comes from the application's name on Discord.
    /// - Line 2 comes from [`details`](Activity::details), and should describe
    ///   what the *player* is doing — dynamic data such as a match score is fine.
    /// - Line 3 describes the *party*: the first half comes from
    ///   [`state`](Activity::state) and the second half from
    ///   [`party`](Activity::party).
    ///
    /// # Invites
    ///
    /// To make an activity joinable, give it an [`ActivityParty`] and an
    /// [`ActivitySecrets`] carrying a join secret. When another user accepts the
    /// invite, that same secret is handed back to their game, which can use it to
    /// join — for example by passing it to `Client::create_or_join_lobby`.
    ///
    /// Further reading: the [rich presence overview] and [best practices].
    ///
    /// [rich presence overview]: https://discord.com/developers/docs/rich-presence/overview
    /// [best practices]: https://discord.com/developers/docs/rich-presence/best-practices
    Activity(sys::Discord_Activity) {
        init: sys::Discord_Activity_Init,
        drop: sys::Discord_Activity_Drop,
        clone: sys::Discord_Activity_Clone,
    }
}

impl Activity {
    /// Add a custom button to the rich presence.
    ///
    /// The button is copied, so `button` remains usable afterwards.
    pub fn add_button(&mut self, button: &ActivityButton) {
        // SAFETY: `button` is an initialised handle kept alive across the call;
        // the SDK copies it rather than taking ownership.
        unsafe { sys::Discord_Activity_AddButton(self.as_raw_mut(), button.as_raw()) }
    }

    /// The custom buttons on the rich presence.
    pub fn buttons(&self) -> Vec<ActivityButton> {
        // SAFETY: the getter fills the span and transfers ownership of both the
        // array and every element in it, which `span::out` then adopts.
        unsafe {
            span::out(
                |p| sys::Discord_Activity_GetButtons(self.raw_ptr(), p),
                |raw| ActivityButton::from_raw(raw),
            )
        }
    }

    /// The name of the game or application the activity is associated with.
    ///
    /// Defaults to the name of the current game.
    pub fn name(&self) -> String {
        // SAFETY: read-only getter that transfers ownership of the string to us.
        unsafe { string::out(|p| sys::Discord_Activity_Name(self.raw_ptr(), p)) }
    }

    /// Set the name of the game or application.
    pub fn set_name(&mut self, value: &str) {
        // SAFETY: the string is passed by value and copied during the call.
        unsafe { sys::Discord_Activity_SetName(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The type of activity this is.
    pub fn activity_type(&self) -> ActivityType {
        // SAFETY: read-only getter on an initialised handle.
        ActivityType::from_raw(unsafe { sys::Discord_Activity_Type(self.raw_ptr()) })
    }

    /// Set the type of activity; this should almost always be
    /// [`ActivityType::Playing`].
    pub fn set_activity_type(&mut self, value: ActivityType) {
        // SAFETY: passing a plain enum value to a setter on an initialised handle.
        unsafe { sys::Discord_Activity_SetType(self.as_raw_mut(), value.into_raw()) }
    }

    /// Which field is used for the user's status message, if one is chosen.
    pub fn status_display_type(&self) -> Option<StatusDisplayType> {
        // SAFETY: read-only getter that initialises the out-parameter only when
        // it returns `true`.
        unsafe { opt_out(|p| sys::Discord_Activity_StatusDisplayType(self.raw_ptr(), p)) }
            .map(StatusDisplayType::from_raw)
    }

    /// Set which field is used for the user's status message, or clear the
    /// choice with `None`.
    pub fn set_status_display_type(&mut self, value: Option<StatusDisplayType>) {
        match value {
            Some(v) => {
                let mut raw = v.into_raw();
                // SAFETY: `raw` outlives the call and the SDK reads through the
                // pointer without retaining it.
                unsafe { sys::Discord_Activity_SetStatusDisplayType(self.as_raw_mut(), &mut raw) }
            }
            // SAFETY: a null pointer is the SDK's "clear this field" signal.
            None => unsafe {
                sys::Discord_Activity_SetStatusDisplayType(self.as_raw_mut(), std::ptr::null_mut())
            },
        }
    }

    /// The state *of the party* for this activity.
    pub fn state(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_Activity_State(self.raw_ptr(), p)) }
    }

    /// Set the party state, or clear it with `None`.
    ///
    /// If specified, must be between 2 and 128 characters.
    pub fn set_state(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_Activity_SetState(self.as_raw_mut(), p)
        })
    }

    /// A URL that opens when the user clicks or taps the state text.
    pub fn state_url(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_Activity_StateUrl(self.raw_ptr(), p)) }
    }

    /// Set the state link, or clear it with `None`.
    ///
    /// If specified, must be between 2 and 256 characters.
    pub fn set_state_url(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_Activity_SetStateUrl(self.as_raw_mut(), p)
        })
    }

    /// The state *of what the user is doing* for this activity.
    pub fn details(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_Activity_Details(self.raw_ptr(), p)) }
    }

    /// Set the details, or clear them with `None`.
    ///
    /// If specified, must be between 2 and 128 characters.
    pub fn set_details(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_Activity_SetDetails(self.as_raw_mut(), p)
        })
    }

    /// A URL that opens when the user clicks or taps the details text.
    pub fn details_url(&self) -> Option<String> {
        // SAFETY: read-only getter transferring ownership of the string.
        unsafe { string::out_opt(|p| sys::Discord_Activity_DetailsUrl(self.raw_ptr(), p)) }
    }

    /// Set the details link, or clear it with `None`.
    ///
    /// If specified, must be between 2 and 256 characters.
    pub fn set_details_url(&mut self, value: Option<&str>) {
        // SAFETY: the SDK copies the string; null clears the field.
        string::with_opt(value, |p| unsafe {
            sys::Discord_Activity_SetDetailsUrl(self.as_raw_mut(), p)
        })
    }

    /// The application id of the game the activity is associated with.
    ///
    /// This is always the application id of the current game, or of a game from
    /// the same publisher; it cannot meaningfully be chosen by the game.
    pub fn application_id(&self) -> Option<u64> {
        // SAFETY: read-only getter that initialises the out-parameter only when
        // it returns `true`.
        unsafe { opt_out(|p| sys::Discord_Activity_ApplicationId(self.raw_ptr(), p)) }
    }

    /// Set the application id, or clear it with `None`.
    pub fn set_application_id(&mut self, value: Option<u64>) {
        match value {
            // SAFETY: `v` outlives the call and the SDK only reads through it.
            Some(mut v) => unsafe {
                sys::Discord_Activity_SetApplicationId(self.as_raw_mut(), &mut v)
            },
            // SAFETY: a null pointer is the SDK's "clear this field" signal.
            None => unsafe {
                sys::Discord_Activity_SetApplicationId(self.as_raw_mut(), std::ptr::null_mut())
            },
        }
    }

    /// The application id of the parent application, if the game has one.
    ///
    /// Identifies a collection of games from the same publisher. Like
    /// [`application_id`](Activity::application_id), this is filled in by the SDK
    /// rather than chosen by the game.
    pub fn parent_application_id(&self) -> Option<u64> {
        // SAFETY: read-only getter that initialises the out-parameter only when
        // it returns `true`.
        unsafe { opt_out(|p| sys::Discord_Activity_ParentApplicationId(self.raw_ptr(), p)) }
    }

    /// Set the parent application id, or clear it with `None`.
    pub fn set_parent_application_id(&mut self, value: Option<u64>) {
        match value {
            // SAFETY: `v` outlives the call and the SDK only reads through it.
            Some(mut v) => unsafe {
                sys::Discord_Activity_SetParentApplicationId(self.as_raw_mut(), &mut v)
            },
            // SAFETY: a null pointer is the SDK's "clear this field" signal.
            None => unsafe {
                sys::Discord_Activity_SetParentApplicationId(self.as_raw_mut(), std::ptr::null_mut())
            },
        }
    }

    /// The images used to customise how the activity is displayed.
    pub fn assets(&self) -> Option<ActivityAssets> {
        // SAFETY: the getter writes an initialised handle we own into the
        // out-parameter, but only when it returns `true`.
        unsafe {
            opt_out(|p| sys::Discord_Activity_Assets(self.raw_ptr(), p))
                .map(|raw| ActivityAssets::from_raw(raw))
        }
    }

    /// Set the activity's images, or clear them with `None`.
    ///
    /// The handle is copied, so `value` remains usable afterwards.
    pub fn set_assets(&mut self, value: Option<&ActivityAssets>) {
        let ptr = value.map_or(std::ptr::null_mut(), |v| v.raw_ptr());
        // SAFETY: `ptr` is either null — meaning "clear" — or points at an
        // initialised handle that outlives the call.
        unsafe { sys::Discord_Activity_SetAssets(self.as_raw_mut(), ptr) }
    }

    /// The activity's start and end times.
    pub fn timestamps(&self) -> Option<ActivityTimestamps> {
        // SAFETY: the getter writes an initialised handle we own into the
        // out-parameter, but only when it returns `true`.
        unsafe {
            opt_out(|p| sys::Discord_Activity_Timestamps(self.raw_ptr(), p))
                .map(|raw| ActivityTimestamps::from_raw(raw))
        }
    }

    /// Set the activity's timestamps, or clear them with `None`.
    ///
    /// The handle is copied, so `value` remains usable afterwards.
    pub fn set_timestamps(&mut self, value: Option<&ActivityTimestamps>) {
        let ptr = value.map_or(std::ptr::null_mut(), |v| v.raw_ptr());
        // SAFETY: `ptr` is either null — meaning "clear" — or points at an
        // initialised handle that outlives the call.
        unsafe { sys::Discord_Activity_SetTimestamps(self.as_raw_mut(), ptr) }
    }

    /// The party the user is playing with.
    pub fn party(&self) -> Option<ActivityParty> {
        // SAFETY: the getter writes an initialised handle we own into the
        // out-parameter, but only when it returns `true`.
        unsafe {
            opt_out(|p| sys::Discord_Activity_Party(self.raw_ptr(), p))
                .map(|raw| ActivityParty::from_raw(raw))
        }
    }

    /// Set the activity's party, or clear it with `None`.
    ///
    /// The handle is copied, so `value` remains usable afterwards.
    pub fn set_party(&mut self, value: Option<&ActivityParty>) {
        let ptr = value.map_or(std::ptr::null_mut(), |v| v.raw_ptr());
        // SAFETY: `ptr` is either null — meaning "clear" — or points at an
        // initialised handle that outlives the call.
        unsafe { sys::Discord_Activity_SetParty(self.as_raw_mut(), ptr) }
    }

    /// The secrets that make the activity joinable.
    pub fn secrets(&self) -> Option<ActivitySecrets> {
        // SAFETY: the getter writes an initialised handle we own into the
        // out-parameter, but only when it returns `true`.
        unsafe {
            opt_out(|p| sys::Discord_Activity_Secrets(self.raw_ptr(), p))
                .map(|raw| ActivitySecrets::from_raw(raw))
        }
    }

    /// Set the activity's secrets, or clear them with `None`.
    ///
    /// The handle is copied, so `value` remains usable afterwards.
    pub fn set_secrets(&mut self, value: Option<&ActivitySecrets>) {
        let ptr = value.map_or(std::ptr::null_mut(), |v| v.raw_ptr());
        // SAFETY: `ptr` is either null — meaning "clear" — or points at an
        // initialised handle that outlives the call.
        unsafe { sys::Discord_Activity_SetSecrets(self.as_raw_mut(), ptr) }
    }

    /// The platforms the activity is joinable on.
    pub fn supported_platforms(&self) -> ActivityGamePlatforms {
        // SAFETY: read-only getter on an initialised handle.
        let raw = unsafe { sys::Discord_Activity_SupportedPlatforms(self.raw_ptr()) };
        ActivityGamePlatforms::from_bits(raw.0)
    }

    /// Set the platforms the activity is joinable on.
    ///
    /// Useful when an activity is joinable only from some platforms — if PC users
    /// cannot join mobile users and vice versa, this makes the activity show as
    /// joinable on Discord only for users on a compatible platform.
    pub fn set_supported_platforms(&mut self, value: ActivityGamePlatforms) {
        let raw = sys::Discord_ActivityGamePlatforms(value.bits());
        // SAFETY: passing a plain bitmask to a setter on an initialised handle.
        unsafe { sys::Discord_Activity_SetSupportedPlatforms(self.as_raw_mut(), raw) }
    }
}

/// Field-by-field comparison, as performed by the SDK itself.
impl PartialEq for Activity {
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: both handles are initialised and only read by the comparison.
        unsafe { sys::Discord_Activity_Equals(self.raw_ptr(), other.as_raw()) }
    }
}

impl std::fmt::Debug for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Activity")
            .field("name", &self.name())
            .field("activity_type", &self.activity_type())
            .field("status_display_type", &self.status_display_type())
            .field("state", &self.state())
            .field("state_url", &self.state_url())
            .field("details", &self.details())
            .field("details_url", &self.details_url())
            .field("application_id", &self.application_id())
            .field("parent_application_id", &self.parent_application_id())
            .field("assets", &self.assets())
            .field("timestamps", &self.timestamps())
            .field("party", &self.party())
            .field("secrets", &self.secrets())
            .field("supported_platforms", &self.supported_platforms())
            .field("buttons", &self.buttons())
            .finish()
    }
}
