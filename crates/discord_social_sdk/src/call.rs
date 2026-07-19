//! Active voice sessions.
//!
//! [`Call`] manages an active voice session in a lobby. It is handed back by the
//! `Client` when joining a lobby's voice channel, and stays useful for as long
//! as the session lasts: muting, deafening, per-participant volume, push to talk
//! and voice auto detection all live here.
//!
//! A call is **not ready to be used until [`Call::status`] reaches
//! [`CallStatus::Connected`]**, so most integrations install
//! [`Call::on_status_changed`] first and drive the rest from there.
//!
//! [`CallInfo`] is a lighter, read-only summary of a call in a lobby, and
//! [`VoiceState`] reports whether an individual participant has muted or
//! deafened themselves.
//!
//! # Events
//!
//! The `on_*` methods install persistent event handlers. Each replaces any
//! previously installed handler for that event, and the closure lives until it
//! is replaced or the [`Call`] is dropped. Like everything else in the SDK,
//! handlers only fire from [`run_callbacks`](crate::run_callbacks).

use std::ffi::c_void;
use std::mem::MaybeUninit;

use discord_social_sdk_sys as sys;

use crate::audio::VadThresholdSettings;
use crate::callback;
use crate::enums::{AudioModeType, CallError, CallStatus};
use crate::handle::handle;
use crate::span;
use crate::string;

handle! {
    /// The state of a single participant in a Discord voice call.
    ///
    /// The main use case is to communicate whether a user has muted or deafened
    /// themselves.
    ///
    /// Handle objects hold a reference both to the underlying data and to the
    /// SDK instance, so changes to the underlying data are generally visible on
    /// existing handles without re-creating them. If the SDK instance is
    /// destroyed while a handle is still alive, every method returns its default
    /// value.
    VoiceState(sys::Discord_VoiceStateHandle) {
        drop: sys::Discord_VoiceStateHandle_Drop,
        clone: sys::Discord_VoiceStateHandle_Clone,
    }
}

impl VoiceState {
    /// Whether the user has deafened themselves.
    ///
    /// A deafened user cannot be heard by anyone else in the call, and does not
    /// hear anyone else in the call either.
    pub fn self_deaf(&self) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_VoiceStateHandle_SelfDeaf(self.raw_ptr()) }
    }

    /// Whether the user has muted themselves so that no one else in the call can
    /// hear them.
    pub fn self_mute(&self) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_VoiceStateHandle_SelfMute(self.raw_ptr()) }
    }
}

impl std::fmt::Debug for VoiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceState")
            .field("self_deaf", &self.self_deaf())
            .field("self_mute", &self.self_mute())
            .finish()
    }
}

handle! {
    /// A summary of the state of a single Discord call in a lobby.
    ///
    /// This is the read-only counterpart to [`Call`]: it reports who is in the
    /// call and what their voice states are, but cannot change anything.
    CallInfo(sys::Discord_CallInfoHandle) {
        drop: sys::Discord_CallInfoHandle_Drop,
        clone: sys::Discord_CallInfoHandle_Clone,
    }
}

impl CallInfo {
    /// The lobby ID of the call.
    pub fn channel_id(&self) -> u64 {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_CallInfoHandle_ChannelId(self.raw_ptr()) }
    }

    /// The lobby ID of the call.
    pub fn guild_id(&self) -> u64 {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_CallInfoHandle_GuildId(self.raw_ptr()) }
    }

    /// The user IDs of the participants in the call.
    pub fn participants(&self) -> Vec<u64> {
        // SAFETY: the getter fills the span and transfers its backing array to us.
        unsafe {
            span::out(
                |out| sys::Discord_CallInfoHandle_GetParticipants(self.raw_ptr(), out),
                |raw| raw,
            )
        }
    }

    /// The voice state for a single user, so you can know whether they have
    /// muted or deafened themselves.
    ///
    /// Returns [`None`] if the user is not a participant in this call.
    pub fn voice_state(&self, user_id: u64) -> Option<VoiceState> {
        let mut raw = MaybeUninit::<sys::Discord_VoiceStateHandle>::uninit();
        // SAFETY: the out-parameter is only initialised — and only then owned by
        // us — when the getter returns true.
        unsafe {
            if sys::Discord_CallInfoHandle_GetVoiceStateHandle(
                self.raw_ptr(),
                user_id,
                raw.as_mut_ptr(),
            ) {
                Some(VoiceState::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }
}

impl std::fmt::Debug for CallInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallInfo")
            .field("channel_id", &self.channel_id())
            .field("guild_id", &self.guild_id())
            .field("participants", &self.participants())
            .finish()
    }
}

handle! {
    /// An active voice session in a lobby.
    ///
    /// A call is not ready to be used until [`status`](Call::status) changes to
    /// [`CallStatus::Connected`]; install [`on_status_changed`](Call::on_status_changed)
    /// to observe that transition.
    Call(sys::Discord_Call) {
        drop: sys::Discord_Call_Drop,
    }
}

impl Clone for Call {
    fn clone(&self) -> Self {
        let mut raw = MaybeUninit::<sys::Discord_Call>::uninit();
        // SAFETY: argument order is (destination, source). Unlike every other
        // handle in the SDK, `Discord_Call_Clone` declares its source parameter
        // non-const, so `raw_ptr` is used instead of a shared borrow; the SDK
        // only reads through it.
        unsafe {
            sys::Discord_Call_Clone(raw.as_mut_ptr(), self.raw_ptr());
            Self {
                raw: raw.assume_init(),
            }
        }
    }
}

impl Call {
    /// Render a [`CallStatus`] as the string the SDK uses for it.
    pub fn status_to_string(status: CallStatus) -> String {
        // SAFETY: the conversion fills the out-parameter and transfers the buffer to us.
        unsafe { string::out(|out| sys::Discord_Call_StatusToString(status.into_raw(), out)) }
    }

    /// Render a [`CallError`] as the string the SDK uses for it.
    pub fn error_to_string(error: CallError) -> String {
        // SAFETY: the conversion fills the out-parameter and transfers the buffer to us.
        unsafe { string::out(|out| sys::Discord_Call_ErrorToString(error.into_raw(), out)) }
    }

    /// Whether the call is configured to use voice auto detection or push to
    /// talk for the current user.
    pub fn audio_mode(&self) -> AudioModeType {
        // SAFETY: a read-only getter on a live handle.
        AudioModeType::from_raw(unsafe { sys::Discord_Call_GetAudioMode(self.raw_ptr()) })
    }

    /// Set whether to use voice auto detection or push to talk for the current
    /// user on this call.
    ///
    /// If using push to talk you should call
    /// [`set_ptt_active`](Call::set_ptt_active) whenever the user presses their
    /// configured push to talk key.
    pub fn set_audio_mode(&mut self, audio_mode: AudioModeType) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetAudioMode(self.as_raw_mut(), audio_mode.into_raw()) }
    }

    /// The ID of the lobby with which this call is associated.
    pub fn channel_id(&self) -> u64 {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetChannelId(self.raw_ptr()) }
    }

    /// The ID of the lobby with which this call is associated.
    pub fn guild_id(&self) -> u64 {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetGuildId(self.raw_ptr()) }
    }

    /// Whether the current user has locally muted `user_id` for themselves.
    pub fn local_mute(&self, user_id: u64) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetLocalMute(self.raw_ptr(), user_id) }
    }

    /// Locally mute `user_id`, so that the current user cannot hear them anymore.
    ///
    /// Does not affect whether the given user is muted for any other connected
    /// client.
    pub fn set_local_mute(&mut self, user_id: u64, mute: bool) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetLocalMute(self.as_raw_mut(), user_id, mute) }
    }

    /// The user IDs of all the participants in the call.
    pub fn participants(&self) -> Vec<u64> {
        // SAFETY: the getter fills the span and transfers its backing array to us.
        unsafe {
            span::out(
                |out| sys::Discord_Call_GetParticipants(self.raw_ptr(), out),
                |raw| raw,
            )
        }
    }

    /// The locally set playout volume of `user_id`.
    ///
    /// The range of volume is `[0, 200]`, where `100` indicates the default
    /// audio volume of the playback device.
    pub fn participant_volume(&self, user_id: u64) -> f32 {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetParticipantVolume(self.raw_ptr(), user_id) }
    }

    /// Locally change the playout volume of `user_id`.
    ///
    /// Does not affect the volume of this user for any other connected client.
    /// The range of volume is `[0, 200]`, where `100` indicates the default
    /// audio volume of the playback device.
    pub fn set_participant_volume(&mut self, user_id: u64, volume: f32) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetParticipantVolume(self.as_raw_mut(), user_id, volume) }
    }

    /// Whether push to talk is currently active, meaning the user is currently
    /// pressing their configured push to talk key.
    pub fn ptt_active(&self) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetPTTActive(self.raw_ptr()) }
    }

    /// Report that the user pushed or released their push to talk key.
    ///
    /// When push to talk is enabled this should be called on every press and
    /// release. The key must be configured in the game — the SDK does not handle
    /// keybinds itself.
    pub fn set_ptt_active(&mut self, active: bool) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetPTTActive(self.as_raw_mut(), active) }
    }

    /// The time, in milliseconds, that push to talk stays active after the user
    /// releases the key and `set_ptt_active(false)` is called.
    pub fn ptt_release_delay(&self) -> u32 {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetPTTReleaseDelay(self.raw_ptr()) }
    }

    /// Extend the time that push to talk stays active after the user releases
    /// the key and `set_ptt_active(false)` is called.
    ///
    /// Defaults to no release delay; Discord itself uses 20ms, which is the
    /// recommended value.
    pub fn set_ptt_release_delay(&mut self, release_delay_ms: u32) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetPTTReleaseDelay(self.as_raw_mut(), release_delay_ms) }
    }

    /// Whether the current user is deafened.
    pub fn self_deaf(&self) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetSelfDeaf(self.raw_ptr()) }
    }

    /// Mute all audio from the currently active call for the current user.
    ///
    /// They will not be able to hear any other participant, and no other
    /// participant will be able to hear them either.
    pub fn set_self_deaf(&mut self, deaf: bool) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetSelfDeaf(self.as_raw_mut(), deaf) }
    }

    /// Whether the current user's microphone is muted.
    pub fn self_mute(&self) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_Call_GetSelfMute(self.raw_ptr()) }
    }

    /// Mute the current user's microphone so that no other participant in their
    /// active calls can hear them.
    pub fn set_self_mute(&mut self, mute: bool) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetSelfMute(self.as_raw_mut(), mute) }
    }

    /// The current call status.
    ///
    /// A call is not ready to be used until this reaches
    /// [`CallStatus::Connected`].
    pub fn status(&self) -> CallStatus {
        // SAFETY: a read-only getter on a live handle.
        CallStatus::from_raw(unsafe { sys::Discord_Call_GetStatus(self.raw_ptr()) })
    }

    /// The current configuration for voice auto detection thresholds.
    ///
    /// See [`VadThresholdSettings`] for the meaning of the individual fields.
    pub fn vad_threshold(&self) -> VadThresholdSettings {
        let mut raw = MaybeUninit::<sys::Discord_VADThresholdSettings>::uninit();
        // SAFETY: the getter fully initialises the out-parameter and transfers
        // ownership of the settings handle to us.
        unsafe {
            sys::Discord_Call_GetVADThreshold(self.raw_ptr(), raw.as_mut_ptr());
            VadThresholdSettings::from_raw(raw.assume_init())
        }
    }

    /// Customise the voice auto detection threshold for picking up activity from
    /// the user's microphone.
    ///
    /// When `automatic` is `true`, Discord automatically detects the appropriate
    /// threshold to use and `threshold` is ignored. When it is `false`, the
    /// given `threshold` is used; it has a range of `-100` to `0` and defaults
    /// to `-60`.
    pub fn set_vad_threshold(&mut self, automatic: bool, threshold: f32) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_Call_SetVADThreshold(self.as_raw_mut(), automatic, threshold) }
    }

    /// The voice state for `user_id`, one of this call's participants.
    ///
    /// The [`VoiceState`] allows other users to know whether the target user has
    /// muted or deafened themselves. Returns [`None`] if the user is not a
    /// participant in this call.
    pub fn voice_state(&self, user_id: u64) -> Option<VoiceState> {
        let mut raw = MaybeUninit::<sys::Discord_VoiceStateHandle>::uninit();
        // SAFETY: the out-parameter is only initialised — and only then owned by
        // us — when the getter returns true.
        unsafe {
            if sys::Discord_Call_GetVoiceStateHandle(self.raw_ptr(), user_id, raw.as_mut_ptr()) {
                Some(VoiceState::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Install a handler invoked whenever a field on a [`VoiceState`] for a user
    /// would have changed.
    ///
    /// For example when a user mutes themselves, all other connected clients
    /// invoke this callback, because the "self mute" field is now true. It is
    /// generally *not* invoked when users join or leave channels — use
    /// [`on_participant_changed`](Call::on_participant_changed) for that.
    ///
    /// The closure receives the ID of the user whose voice state changed.
    /// Replaces any previously installed handler.
    pub fn on_voice_state_changed<F>(&mut self, callback: F)
    where
        F: FnMut(u64) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(u64) + 'static>(user_id: u64, userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below, still owned by the SDK.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(user_id)) }
        }
        // SAFETY: the SDK stores the boxed closure and releases it through `free_fn::<F>()`.
        unsafe {
            sys::Discord_Call_SetOnVoiceStateChangedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Install a handler invoked whenever someone joins or leaves the voice call.
    ///
    /// The closure receives the user's ID and `true` if they were added to the
    /// call, `false` if they left. Replaces any previously installed handler.
    pub fn on_participant_changed<F>(&mut self, callback: F)
    where
        F: FnMut(u64, bool) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(u64, bool) + 'static>(
            user_id: u64,
            added: bool,
            userdata: *mut c_void,
        ) {
            // SAFETY: `userdata` is the boxed `F` installed below, still owned by the SDK.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(user_id, added)) }
        }
        // SAFETY: the SDK stores the boxed closure and releases it through `free_fn::<F>()`.
        unsafe {
            sys::Discord_Call_SetParticipantChangedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Install a handler invoked whenever a user starts or stops speaking.
    ///
    /// The closure receives the user's ID and whether they are currently playing
    /// sound. It can be invoked in other cases as well, such as when the
    /// priority speaker changes or when the user plays a soundboard sound.
    /// Replaces any previously installed handler.
    pub fn on_speaking_status_changed<F>(&mut self, callback: F)
    where
        F: FnMut(u64, bool) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(u64, bool) + 'static>(
            user_id: u64,
            is_playing_sound: bool,
            userdata: *mut c_void,
        ) {
            // SAFETY: `userdata` is the boxed `F` installed below, still owned by the SDK.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(user_id, is_playing_sound)) }
        }
        // SAFETY: the SDK stores the boxed closure and releases it through `free_fn::<F>()`.
        unsafe {
            sys::Discord_Call_SetSpeakingStatusChangedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Install a handler invoked when the call status changes, such as when it
    /// fully connects or starts reconnecting.
    ///
    /// The closure receives the new [`CallStatus`], any [`CallError`] that
    /// caused it, and an error detail code whose meaning depends on the error.
    /// Replaces any previously installed handler.
    pub fn on_status_changed<F>(&mut self, callback: F)
    where
        F: FnMut(CallStatus, CallError, i32) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(CallStatus, CallError, i32) + 'static>(
            status: sys::Discord_Call_Status,
            error: sys::Discord_Call_Error,
            error_detail: i32,
            userdata: *mut c_void,
        ) {
            // SAFETY: `userdata` is the boxed `F` installed below, still owned by the SDK.
            unsafe {
                callback::dispatch_mut::<F>(userdata, |f| {
                    f(
                        CallStatus::from_raw(status),
                        CallError::from_raw(error),
                        error_detail,
                    )
                })
            }
        }
        // SAFETY: the SDK stores the boxed closure and releases it through `free_fn::<F>()`.
        unsafe {
            sys::Discord_Call_SetStatusChangedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }
}

impl std::fmt::Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("channel_id", &self.channel_id())
            .field("guild_id", &self.guild_id())
            .field("status", &self.status())
            .field("audio_mode", &self.audio_mode())
            .field("self_mute", &self.self_mute())
            .field("self_deaf", &self.self_deaf())
            .field("participants", &self.participants())
            .finish()
    }
}
