//! Rust enums mirroring the SDK's C enums.
//!
//! bindgen renders C enums as newtype constants, which are neither exhaustive
//! nor matchable. These wrappers restore real `match` support.
//!
//! Every C enum carries a `_forceint = 0x7FFFFFFF` member that exists only to
//! pin the underlying integer width — it is never a real value, so it is not
//! reproduced here. Unrecognised values map to an `Unknown` variant instead of
//! panicking, since the SDK may add values in a newer runtime than this crate
//! was built against.

use discord_social_sdk_sys as sys;

/// Define a Rust enum mirroring a C enum, with conversions in both directions.
///
/// Generates the enum, `from_raw`/`into_raw`, and `From` impls. Values the
/// binding does not know become `Unknown(raw)`, keeping forward compatibility
/// with newer SDK runtimes.
macro_rules! c_enum {
    (
        $(#[$meta:meta])*
        $name:ident : $raw:path {
            $(
                $(#[$vmeta:meta])*
                $variant:ident = $konst:path
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $(
                $(#[$vmeta])*
                $variant,
            )*
            /// A value this binding does not recognise.
            Unknown(i32),
        }

        impl $name {
            /// Convert from the raw C value.
            pub fn from_raw(raw: $raw) -> Self {
                match raw {
                    $($konst => Self::$variant,)*
                    other => Self::Unknown(other.0 as i32),
                }
            }

            /// Convert back to the raw C value.
            pub fn into_raw(self) -> $raw {
                match self {
                    $(Self::$variant => $konst,)*
                    Self::Unknown(v) => $raw(v as _),
                }
            }
        }

        impl From<$raw> for $name {
            fn from(raw: $raw) -> Self {
                Self::from_raw(raw)
            }
        }

        impl From<$name> for $raw {
            fn from(value: $name) -> Self {
                value.into_raw()
            }
        }
    };
}

pub(crate) use c_enum;

c_enum! {
    /// How a user is being invited to an activity.
    ActivityActionType: sys::Discord_ActivityActionTypes {
        /// An invitation to join the sender's party.
        Invalid = sys::Discord_ActivityActionTypes::Discord_ActivityActionTypes_Invalid,
        /// An invitation to join the sender's party.
        Join = sys::Discord_ActivityActionTypes::Discord_ActivityActionTypes_Join,
        /// A request to join the recipient's party.
        JoinRequest = sys::Discord_ActivityActionTypes::Discord_ActivityActionTypes_JoinRequest,
    }
}

c_enum! {
    /// Whether a party is joinable by anyone or only by invitation.
    ActivityPartyPrivacy: sys::Discord_ActivityPartyPrivacy {
        /// Only invited users may join.
        Private = sys::Discord_ActivityPartyPrivacy::Discord_ActivityPartyPrivacy_Private,
        /// Anyone may join.
        Public = sys::Discord_ActivityPartyPrivacy::Discord_ActivityPartyPrivacy_Public,
    }
}

c_enum! {
    /// The kind of activity a user is engaged in.
    ///
    /// Discord rich presence supports several activity types; for the SDK the
    /// only really relevant one is [`Playing`](ActivityType::Playing). The rest
    /// are provided for completeness.
    ///
    /// See the [rich presence overview] for more information.
    ///
    /// [rich presence overview]: https://discord.com/developers/docs/rich-presence/overview
    ActivityType: sys::Discord_ActivityTypes {
        /// The user is playing a game.
        Playing = sys::Discord_ActivityTypes::Discord_ActivityTypes_Playing,
        /// The user is streaming.
        Streaming = sys::Discord_ActivityTypes::Discord_ActivityTypes_Streaming,
        /// The user is listening to something.
        Listening = sys::Discord_ActivityTypes::Discord_ActivityTypes_Listening,
        /// The user is watching something.
        Watching = sys::Discord_ActivityTypes::Discord_ActivityTypes_Watching,
        /// The activity is the user's custom status.
        CustomStatus = sys::Discord_ActivityTypes::Discord_ActivityTypes_CustomStatus,
        /// The user is competing.
        Competing = sys::Discord_ActivityTypes::Discord_ActivityTypes_Competing,
        /// The activity is a hang status.
        HangStatus = sys::Discord_ActivityTypes::Discord_ActivityTypes_HangStatus,
    }
}

c_enum! {
    /// Which rich presence field is shown in the user's status.
    ///
    /// See the [rich presence overview] for more information.
    ///
    /// [rich presence overview]: https://discord.com/developers/docs/rich-presence/overview
    StatusDisplayType: sys::Discord_StatusDisplayTypes {
        /// Display the activity's name.
        Name = sys::Discord_StatusDisplayTypes::Discord_StatusDisplayTypes_Name,
        /// Display the activity's state.
        State = sys::Discord_StatusDisplayTypes::Discord_StatusDisplayTypes_State,
        /// Display the activity's details.
        Details = sys::Discord_StatusDisplayTypes::Discord_StatusDisplayTypes_Details,
    }
}

c_enum! {
    /// A platform an activity invite can be accepted on.
    ///
    /// These are **bit flags**: the underlying values are powers of two and the
    /// SDK combines them into a single integer. A raw value carrying more than
    /// one platform will not match any variant here and becomes `Unknown`. Use
    /// [`ActivityGamePlatforms`] to work with combinations.
    ActivityGamePlatform: sys::Discord_ActivityGamePlatforms {
        /// Desktop.
        Desktop = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_Desktop,
        /// Xbox.
        Xbox = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_Xbox,
        /// Samsung.
        Samsung = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_Samsung,
        /// iOS.
        Ios = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_IOS,
        /// Android.
        Android = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_Android,
        /// Embedded (an activity running inside Discord itself).
        Embedded = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_Embedded,
        /// PlayStation 4.
        Ps4 = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_PS4,
        /// PlayStation 5.
        Ps5 = sys::Discord_ActivityGamePlatforms::Discord_ActivityGamePlatforms_PS5,
    }
}

/// A set of [`ActivityGamePlatform`] flags.
///
/// The SDK passes activity invite platforms as a bitwise-or of the individual
/// [`ActivityGamePlatform`] values. This is a thin, allocation-free wrapper over
/// that integer.
///
/// ```
/// use discord_social_sdk::enums::{ActivityGamePlatform, ActivityGamePlatforms};
///
/// let mut set = ActivityGamePlatforms::empty();
/// set.insert(ActivityGamePlatform::Desktop);
/// set.insert(ActivityGamePlatform::Ps5);
/// assert!(set.contains(ActivityGamePlatform::Desktop));
/// assert!(!set.contains(ActivityGamePlatform::Xbox));
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ActivityGamePlatforms(i32);

impl ActivityGamePlatforms {
    /// An empty set.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Wrap a raw bitmask as received from the SDK.
    ///
    /// Bits this binding does not recognise are preserved, so a value round
    /// trips unchanged through [`bits`](Self::bits).
    pub const fn from_bits(bits: i32) -> Self {
        Self(bits)
    }

    /// The raw bitmask, suitable for passing back to the SDK.
    pub const fn bits(self) -> i32 {
        self.0
    }

    /// Whether `platform` is present in the set.
    pub fn contains(self, platform: ActivityGamePlatform) -> bool {
        let bit = platform.into_raw().0;
        bit != 0 && self.0 & bit == bit
    }

    /// Add `platform` to the set.
    pub fn insert(&mut self, platform: ActivityGamePlatform) {
        self.0 |= platform.into_raw().0;
    }
}

c_enum! {
    /// The crypto method used to generate an OAuth2 code challenge.
    ///
    /// The SDK only ever uses SHA-256.
    CodeChallengeMethod: sys::Discord_AuthenticationCodeChallengeMethod {
        /// SHA-256.
        S256 = sys::Discord_AuthenticationCodeChallengeMethod::Discord_AuthenticationCodeChallengeMethod_S256,
    }
}

c_enum! {
    /// How the application is installed.
    IntegrationType: sys::Discord_IntegrationType {
        /// Installed to a guild.
        GuildInstall = sys::Discord_IntegrationType::Discord_IntegrationType_GuildInstall,
        /// Installed to a user account.
        UserInstall = sys::Discord_IntegrationType::Discord_IntegrationType_UserInstall,
    }
}

c_enum! {
    /// The kind of a Discord channel.
    ///
    /// See the [channel resource documentation] for more information.
    ///
    /// [channel resource documentation]: https://discord.com/developers/docs/resources/channel
    ChannelType: sys::Discord_ChannelType {
        /// A text channel in a guild.
        GuildText = sys::Discord_ChannelType::Discord_ChannelType_GuildText,
        /// A direct message between two users.
        Dm = sys::Discord_ChannelType::Discord_ChannelType_Dm,
        /// A voice channel in a guild.
        GuildVoice = sys::Discord_ChannelType::Discord_ChannelType_GuildVoice,
        /// A group direct message.
        GroupDm = sys::Discord_ChannelType::Discord_ChannelType_GroupDm,
        /// A category grouping other guild channels.
        GuildCategory = sys::Discord_ChannelType::Discord_ChannelType_GuildCategory,
        /// A guild announcement channel.
        GuildNews = sys::Discord_ChannelType::Discord_ChannelType_GuildNews,
        /// A guild store channel.
        GuildStore = sys::Discord_ChannelType::Discord_ChannelType_GuildStore,
        /// A thread under an announcement channel.
        GuildNewsThread = sys::Discord_ChannelType::Discord_ChannelType_GuildNewsThread,
        /// A public thread.
        GuildPublicThread = sys::Discord_ChannelType::Discord_ChannelType_GuildPublicThread,
        /// A private thread.
        GuildPrivateThread = sys::Discord_ChannelType::Discord_ChannelType_GuildPrivateThread,
        /// A guild stage voice channel.
        GuildStageVoice = sys::Discord_ChannelType::Discord_ChannelType_GuildStageVoice,
        /// A guild directory channel.
        GuildDirectory = sys::Discord_ChannelType::Discord_ChannelType_GuildDirectory,
        /// A guild forum channel.
        GuildForum = sys::Discord_ChannelType::Discord_ChannelType_GuildForum,
        /// A guild media channel.
        GuildMedia = sys::Discord_ChannelType::Discord_ChannelType_GuildMedia,
        /// An SDK lobby channel.
        Lobby = sys::Discord_ChannelType::Discord_ChannelType_Lobby,
        /// An ephemeral direct message.
        EphemeralDm = sys::Discord_ChannelType::Discord_ChannelType_EphemeralDm,
    }
}

c_enum! {
    /// The kind of additional content attached to a message.
    AdditionalContentType: sys::Discord_AdditionalContentType {
        /// Something other than the listed types.
        Other = sys::Discord_AdditionalContentType::Discord_AdditionalContentType_Other,
        /// A file attachment.
        Attachment = sys::Discord_AdditionalContentType::Discord_AdditionalContentType_Attachment,
        /// A poll.
        Poll = sys::Discord_AdditionalContentType::Discord_AdditionalContentType_Poll,
        /// A voice message.
        VoiceMessage = sys::Discord_AdditionalContentType::Discord_AdditionalContentType_VoiceMessage,
        /// A thread.
        Thread = sys::Discord_AdditionalContentType::Discord_AdditionalContentType_Thread,
        /// An embed.
        Embed = sys::Discord_AdditionalContentType::Discord_AdditionalContentType_Embed,
        /// A sticker.
        Sticker = sys::Discord_AdditionalContentType::Discord_AdditionalContentType_Sticker,
    }
}

c_enum! {
    /// The Discord Voice audio system to use.
    AudioSystem: sys::Discord_AudioSystem {
        /// The standard audio system.
        Standard = sys::Discord_AudioSystem::Discord_AudioSystem_Standard,
        /// The game audio system.
        Game = sys::Discord_AudioSystem::Discord_AudioSystem_Game,
    }
}

c_enum! {
    /// A network error affecting a call.
    CallError: sys::Discord_Call_Error {
        /// No error.
        None = sys::Discord_Call_Error::Discord_Call_Error_None,
        /// The signaling connection could not be established.
        SignalingConnectionFailed = sys::Discord_Call_Error::Discord_Call_Error_SignalingConnectionFailed,
        /// The signaling connection closed unexpectedly.
        SignalingUnexpectedClose = sys::Discord_Call_Error::Discord_Call_Error_SignalingUnexpectedClose,
        /// The voice connection could not be established.
        VoiceConnectionFailed = sys::Discord_Call_Error::Discord_Call_Error_VoiceConnectionFailed,
        /// Joining the call timed out.
        JoinTimeout = sys::Discord_Call_Error::Discord_Call_Error_JoinTimeout,
        /// The user is not permitted to join the call.
        Forbidden = sys::Discord_Call_Error::Discord_Call_Error_Forbidden,
    }
}

c_enum! {
    /// Whether a voice call uses push to talk or automatic voice detection.
    ///
    /// The variant names keep the C enum's `MODE_` prefix, which is part of the
    /// member name rather than the enum prefix.
    AudioModeType: sys::Discord_AudioModeType {
        /// The audio mode has not been initialised yet.
        ModeUninit = sys::Discord_AudioModeType::Discord_AudioModeType_MODE_UNINIT,
        /// Voice activity detection: audio is transmitted when the user speaks.
        ModeVad = sys::Discord_AudioModeType::Discord_AudioModeType_MODE_VAD,
        /// Push to talk: audio is transmitted only while the key is held.
        ModePtt = sys::Discord_AudioModeType::Discord_AudioModeType_MODE_PTT,
    }
}

c_enum! {
    /// The state of a call's network connection.
    CallStatus: sys::Discord_Call_Status {
        /// Not connected.
        Disconnected = sys::Discord_Call_Status::Discord_Call_Status_Disconnected,
        /// Joining the call.
        Joining = sys::Discord_Call_Status::Discord_Call_Status_Joining,
        /// Establishing the connection.
        Connecting = sys::Discord_Call_Status::Discord_Call_Status_Connecting,
        /// The signaling connection is established.
        SignalingConnected = sys::Discord_Call_Status::Discord_Call_Status_SignalingConnected,
        /// Fully connected; voice is flowing.
        Connected = sys::Discord_Call_Status::Discord_Call_Status_Connected,
        /// The connection dropped and is being re-established.
        Reconnecting = sys::Discord_Call_Status::Discord_Call_Status_Reconnecting,
        /// Leaving the call.
        Disconnecting = sys::Discord_Call_Status::Discord_Call_Status_Disconnecting,
    }
}

c_enum! {
    /// The relationship between two users.
    RelationshipType: sys::Discord_RelationshipType {
        /// No relationship with the other user.
        None = sys::Discord_RelationshipType::Discord_RelationshipType_None,
        /// The users are friends.
        Friend = sys::Discord_RelationshipType::Discord_RelationshipType_Friend,
        /// The current user has blocked the target user; actions such as sending
        /// messages between these users will not work.
        Blocked = sys::Discord_RelationshipType::Discord_RelationshipType_Blocked,
        /// A friend request from the target user is awaiting acceptance.
        PendingIncoming = sys::Discord_RelationshipType::Discord_RelationshipType_PendingIncoming,
        /// A friend request to the target user is awaiting acceptance.
        PendingOutgoing = sys::Discord_RelationshipType::Discord_RelationshipType_PendingOutgoing,
        /// Documented for visibility; should be unused in the SDK.
        Implicit = sys::Discord_RelationshipType::Discord_RelationshipType_Implicit,
        /// Documented for visibility; should be unused in the SDK.
        Suggestion = sys::Discord_RelationshipType::Discord_RelationshipType_Suggestion,
    }
}

c_enum! {
    /// The identity provider backing an external account link.
    ///
    /// The C enum's own `Unknown` member is spelled `UnknownProvider` here so it
    /// does not collide with the catch-all `Unknown(raw)` variant the macro adds
    /// for unrecognised values.
    ExternalIdentityProviderType: sys::Discord_ExternalIdentityProviderType {
        /// A generic OpenID Connect provider.
        Oidc = sys::Discord_ExternalIdentityProviderType::Discord_ExternalIdentityProviderType_OIDC,
        /// Epic Online Services.
        EpicOnlineServices = sys::Discord_ExternalIdentityProviderType::Discord_ExternalIdentityProviderType_EpicOnlineServices,
        /// Steam.
        Steam = sys::Discord_ExternalIdentityProviderType::Discord_ExternalIdentityProviderType_Steam,
        /// Unity.
        Unity = sys::Discord_ExternalIdentityProviderType::Discord_ExternalIdentityProviderType_Unity,
        /// A Discord bot.
        DiscordBot = sys::Discord_ExternalIdentityProviderType::Discord_ExternalIdentityProviderType_DiscordBot,
        /// No provider.
        None = sys::Discord_ExternalIdentityProviderType::Discord_ExternalIdentityProviderType_None,
        /// The provider reported by the SDK as unknown.
        UnknownProvider = sys::Discord_ExternalIdentityProviderType::Discord_ExternalIdentityProviderType_Unknown,
    }
}

c_enum! {
    /// The image format to request when building a user's avatar URL.
    AvatarType: sys::Discord_UserHandle_AvatarType {
        /// GIF.
        Gif = sys::Discord_UserHandle_AvatarType::Discord_UserHandle_AvatarType_Gif,
        /// WebP.
        Webp = sys::Discord_UserHandle_AvatarType::Discord_UserHandle_AvatarType_Webp,
        /// PNG.
        Png = sys::Discord_UserHandle_AvatarType::Discord_UserHandle_AvatarType_Png,
        /// JPEG.
        Jpeg = sys::Discord_UserHandle_AvatarType::Discord_UserHandle_AvatarType_Jpeg,
    }
}

c_enum! {
    /// A user's online status.
    ///
    /// Beyond plain online and offline, Discord lets users customise their
    /// status — for example "do not disturb" to silence notifications.
    ///
    /// The C enum's own `Unknown` member is spelled `UnknownStatus` here so it
    /// does not collide with the catch-all `Unknown(raw)` variant the macro adds
    /// for unrecognised values.
    StatusType: sys::Discord_StatusType {
        /// Online and recently active.
        Online = sys::Discord_StatusType::Discord_StatusType_Online,
        /// Offline and not connected to Discord.
        Offline = sys::Discord_StatusType::Discord_StatusType_Offline,
        /// Blocked.
        Blocked = sys::Discord_StatusType::Discord_StatusType_Blocked,
        /// Online but inactive for a while, possibly away from the computer.
        Idle = sys::Discord_StatusType::Discord_StatusType_Idle,
        /// Online but suppressing notifications.
        Dnd = sys::Discord_StatusType::Discord_StatusType_Dnd,
        /// Online but appearing offline to other users.
        Invisible = sys::Discord_StatusType::Discord_StatusType_Invisible,
        /// Online and actively streaming content.
        Streaming = sys::Discord_StatusType::Discord_StatusType_Streaming,
        /// The status reported by the SDK as unknown.
        UnknownStatus = sys::Discord_StatusType::Discord_StatusType_Unknown,
    }
}

c_enum! {
    /// An informational disclosure Discord may make to a user.
    ///
    /// The game can identify these and customise how they are rendered.
    DisclosureType: sys::Discord_DisclosureTypes {
        /// Shown the first time a user sends a message in game, so the user knows
        /// that message will also be viewable on Discord.
        MessageDataVisibleOnDiscord = sys::Discord_DisclosureTypes::Discord_DisclosureTypes_MessageDataVisibleOnDiscord,
    }
}

c_enum! {
    /// An error on the socket connection the SDK maintains with Discord.
    ///
    /// Generic network failures report [`ConnectionFailed`](ClientError::ConnectionFailed)
    /// or [`ConnectionCanceled`](ClientError::ConnectionCanceled). Other errors —
    /// an invalid or expired auth token, for instance — report
    /// [`UnexpectedClose`](ClientError::UnexpectedClose), with the specifics in
    /// the accompanying error details.
    ClientError: sys::Discord_Client_Error {
        /// No error.
        None = sys::Discord_Client_Error::Discord_Client_Error_None,
        /// The connection could not be established.
        ConnectionFailed = sys::Discord_Client_Error::Discord_Client_Error_ConnectionFailed,
        /// The connection closed unexpectedly.
        UnexpectedClose = sys::Discord_Client_Error::Discord_Client_Error_UnexpectedClose,
        /// The connection attempt was canceled.
        ConnectionCanceled = sys::Discord_Client_Error::Discord_Client_Error_ConnectionCanceled,
    }
}

c_enum! {
    /// The status of the SDK's internal websocket connection to Discord.
    ///
    /// Launching the client has roughly two phases:
    ///
    /// 1. The socket connects to Discord and exchanges an auth token —
    ///    [`Connecting`](ClientStatus::Connecting) and
    ///    [`Connected`](ClientStatus::Connected).
    /// 2. The socket receives an initial payload describing the current user,
    ///    their lobbies, their friends and so on — [`Ready`](ClientStatus::Ready).
    ///
    /// Many client functions do not work until the status reaches `Ready`, so
    /// that is the one to wait for.
    ///
    /// The socket may also drop, for example on a temporary network blip; it
    /// reconnects automatically, reporting
    /// [`Reconnecting`](ClientStatus::Reconnecting) meanwhile.
    ClientStatus: sys::Discord_Client_Status {
        /// Not connected.
        Disconnected = sys::Discord_Client_Status::Discord_Client_Status_Disconnected,
        /// Establishing the socket connection.
        Connecting = sys::Discord_Client_Status::Discord_Client_Status_Connecting,
        /// The socket is connected and the auth token has been exchanged.
        Connected = sys::Discord_Client_Status::Discord_Client_Status_Connected,
        /// The initial payload has arrived; the client is fully usable.
        Ready = sys::Discord_Client_Status::Discord_Client_Status_Ready,
        /// The connection dropped and is being re-established.
        Reconnecting = sys::Discord_Client_Status::Discord_Client_Status_Reconnecting,
        /// Tearing the connection down.
        Disconnecting = sys::Discord_Client_Status::Discord_Client_Status_Disconnecting,
        /// Waiting on an HTTP request before continuing.
        HttpWait = sys::Discord_Client_Status::Discord_Client_Status_HttpWait,
    }
}

c_enum! {
    /// An SDK thread whose priority can be controlled.
    ClientThread: sys::Discord_Client_Thread {
        /// The main client thread.
        Client = sys::Discord_Client_Thread::Discord_Client_Thread_Client,
        /// The voice thread.
        Voice = sys::Discord_Client_Thread::Discord_Client_Thread_Voice,
        /// The network thread.
        Network = sys::Discord_Client_Thread::Discord_Client_Thread_Network,
    }
}

c_enum! {
    /// The kind of auth token supplied to the SDK.
    ///
    /// Either the normal token produced by the Discord desktop app or an OAuth2
    /// bearer token. Only the latter can be used by the SDK.
    AuthorizationTokenType: sys::Discord_AuthorizationTokenType {
        /// A Discord desktop app user token.
        User = sys::Discord_AuthorizationTokenType::Discord_AuthorizationTokenType_User,
        /// An OAuth2 bearer token.
        Bearer = sys::Discord_AuthorizationTokenType::Discord_AuthorizationTokenType_Bearer,
    }
}

c_enum! {
    /// An identity provider usable to authenticate a provisional account for a
    /// public client.
    ExternalAuthType: sys::Discord_AuthenticationExternalAuthType {
        /// A generic OpenID Connect token.
        Oidc = sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_OIDC,
        /// An Epic Online Services access token.
        EpicOnlineServicesAccessToken =
            sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_EpicOnlineServicesAccessToken,
        /// An Epic Online Services ID token.
        EpicOnlineServicesIdToken =
            sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_EpicOnlineServicesIdToken,
        /// A Steam session ticket.
        SteamSessionTicket = sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_SteamSessionTicket,
        /// A Unity Services ID token.
        UnityServicesIdToken = sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_UnityServicesIdToken,
        /// An access token issued by a Discord bot.
        DiscordBotIssuedAccessToken =
            sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_DiscordBotIssuedAccessToken,
        /// An Apple ID token.
        AppleIdToken = sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_AppleIdToken,
        /// A PlayStation Network ID token.
        PlayStationNetworkIdToken =
            sys::Discord_AuthenticationExternalAuthType::Discord_AuthenticationExternalAuthType_PlayStationNetworkIdToken,
    }
}

c_enum! {
    /// A log level supported by the SDK.
    LoggingSeverity: sys::Discord_LoggingSeverity {
        /// Verbose diagnostic output.
        Verbose = sys::Discord_LoggingSeverity::Discord_LoggingSeverity_Verbose,
        /// Informational messages.
        Info = sys::Discord_LoggingSeverity::Discord_LoggingSeverity_Info,
        /// Warnings.
        Warning = sys::Discord_LoggingSeverity::Discord_LoggingSeverity_Warning,
        /// Errors.
        Error = sys::Discord_LoggingSeverity::Discord_LoggingSeverity_Error,
        /// Logging disabled.
        None = sys::Discord_LoggingSeverity::Discord_LoggingSeverity_None,
    }
}

c_enum! {
    /// A logical grouping of relationships by online status and game activity.
    RelationshipGroupType: sys::Discord_RelationshipGroupType {
        /// Users who are online and currently playing the game.
        OnlinePlayingGame = sys::Discord_RelationshipGroupType::Discord_RelationshipGroupType_OnlinePlayingGame,
        /// Users who are online but not playing the game.
        OnlineElsewhere = sys::Discord_RelationshipGroupType::Discord_RelationshipGroupType_OnlineElsewhere,
        /// Users who are offline.
        Offline = sys::Discord_RelationshipGroupType::Discord_RelationshipGroupType_Offline,
    }
}
