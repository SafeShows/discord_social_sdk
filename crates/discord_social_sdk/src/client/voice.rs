//! Voice calls, audio devices, and the voice engine's processing settings.
//!
//! # The call model
//!
//! Voice in the Social SDK is per-channel. [`Client::start_call`] joins the voice
//! session for a channel — for a lobby, that is simply the lobby id — and hands
//! back a [`Call`] for as long as the session lasts. A call is not usable until
//! its [`status`](Call::status) reaches
//! [`CallStatus::Connected`](crate::enums::CallStatus::Connected), so install
//! [`Call::on_status_changed`] before doing anything else with it.
//!
//! There can be several calls at once, one per channel.
//! [`Client::calls`] enumerates them, [`Client::call`] looks one up, and
//! [`Client::end_call`] / [`Client::end_calls`] tear them down. **Ending a call
//! invalidates every [`Call`] handle referring to it**; drop them.
//!
//! # Per-call versus global
//!
//! Mute and deafen exist at both levels. [`Call::set_self_mute`] affects one
//! call; [`Client::set_self_mute_all`] affects every call and *overrides* the
//! per-call setting. Muting stops other participants from hearing you; deafening
//! additionally stops you from hearing them.
//!
//! # Devices
//!
//! Discord starts on the system default input and output devices. Enumerate the
//! alternatives with [`Client::input_devices`] / [`Client::output_devices`] and
//! switch with [`Client::set_input_device`] / [`Client::set_output_device`],
//! passing an [`AudioDevice::id`]. [`Client::on_device_change`] fires when the
//! set of available devices changes.
//!
//! # Voice processing
//!
//! Echo cancellation, noise suppression, Krisp noise cancellation, and automatic
//! gain control are all on sensible defaults. The setters here exist mainly for
//! applications that surface a voice-settings UI of their own; they are not part
//! of a normal integration.
//!
//! # Raw audio
//!
//! [`Client::start_call_with_audio_callbacks`] additionally exposes the PCM
//! stream — incoming audio per user, and locally captured audio before it is
//! transmitted. See that method for the buffer contract.

use super::Client;
use crate::audio::AudioDevice;
use crate::call::Call;
use crate::error::{Result, to_result};
use crate::{callback, span, string};
use discord_social_sdk_sys as sys;
use std::ffi::c_void;
use std::mem::MaybeUninit;

/// The shape of one buffer of PCM audio delivered to an audio callback.
///
/// Samples are interleaved 16-bit signed PCM, so the buffer handed to the
/// callback holds `samples_per_channel * channels` values in total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioFrame {
    /// Number of samples per channel in this buffer.
    pub samples_per_channel: u64,
    /// Sample rate in Hz.
    pub sample_rate: i32,
    /// Number of interleaved channels.
    pub channels: u64,
}

impl AudioFrame {
    /// Total number of `i16` samples across all channels.
    fn sample_count(&self) -> usize {
        (self.samples_per_channel as usize).saturating_mul(self.channels as usize)
    }
}

/// Adopt an `AudioDeviceSpan` delivered by value to a callback.
///
/// Despite the callback signature suggesting a borrow, the SDK transfers
/// ownership of both the array and its elements — the official C++ wrapper
/// adopts every element and then calls `Discord_Free` on the array. This does
/// the same.
///
/// # Safety
///
/// `raw` must be a span the SDK just handed to a callback, adopted exactly once.
unsafe fn take_device_span(raw: sys::Discord_AudioDeviceSpan) -> Vec<AudioDevice> {
    // SAFETY: writing the by-value span into the out-parameter satisfies
    // `span::out`'s contract that `f` fully initialises it; ownership of the
    // array and every element transfers to us, as the C++ wrapper assumes.
    unsafe {
        span::out(
            |out: *mut sys::Discord_AudioDeviceSpan| out.write(raw),
            |elem| AudioDevice::from_raw(elem),
        )
    }
}

/// Trampoline shared by the device-selection callbacks, which report only
/// success or failure.
unsafe extern "C" fn result_tramp<F>(result: *mut sys::Discord_ClientResult, userdata: *mut c_void)
where
    F: FnOnce(Result<()>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F` installed alongside this trampoline.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(to_result(result))) }
}

/// Trampoline shared by the two "fetch the current device" callbacks.
unsafe extern "C" fn current_device_tramp<F>(device: *const sys::Discord_AudioDevice, userdata: *mut c_void)
where
    F: FnOnce(Option<AudioDevice>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F`. The SDK transfers ownership of the
    // device handle to us — the C++ wrapper likewise adopts `*device` — so it is
    // read out by value rather than borrowed. A null pointer is reported as
    // `None` instead of being dereferenced.
    unsafe {
        callback::dispatch_once::<F>(userdata, |f| {
            let owned = if device.is_null() {
                None
            } else {
                Some(AudioDevice::from_raw(std::ptr::read(device)))
            };
            f(owned)
        })
    }
}

/// Trampoline shared by the two "list devices" callbacks.
unsafe extern "C" fn device_list_tramp<F>(devices: sys::Discord_AudioDeviceSpan, userdata: *mut c_void)
where
    F: FnOnce(Vec<AudioDevice>) + 'static,
{
    // SAFETY: `userdata` is the boxed `F`; the span is adopted exactly once here.
    unsafe { callback::dispatch_once::<F>(userdata, |f| f(take_device_span(devices))) }
}

impl Client {
    // ---- Starting and ending calls ----

    /// Start or join the call in `channel_id`.
    ///
    /// For a lobby, pass the lobby id. Returns [`None`] if the user is already in
    /// that voice channel.
    ///
    /// The returned [`Call`] is not usable until its status reaches
    /// [`CallStatus::Connected`](crate::enums::CallStatus::Connected); bind
    /// [`Call::on_status_changed`] first.
    ///
    /// # Platform notes
    ///
    /// On iOS the application is responsible for enabling the appropriate
    /// background audio mode in its `Info.plist`. On macOS, set
    /// `NSMicrophoneUsageDescription` in `Info.plist`.
    pub fn start_call(&mut self, channel_id: u64) -> Option<Call> {
        let mut raw = MaybeUninit::<sys::Discord_Call>::uninit();
        // SAFETY: the out-parameter is only initialised — and only then owned by
        // us — when the call reports success.
        unsafe {
            if sys::Discord_Client_StartCall(self.as_raw_mut(), channel_id, raw.as_mut_ptr()) {
                Some(Call::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Start or join the call in `lobby_id`, tapping the raw PCM audio stream.
    ///
    /// Like [`start_call`](Self::start_call), but additionally installs two
    /// persistent audio callbacks. Returns [`None`] if the user is already in
    /// that voice channel.
    ///
    /// `received` is invoked for each buffer of incoming audio, per speaking
    /// user. `captured` is invoked for each buffer of locally captured audio
    /// *before* it is processed and transmitted, which is useful for voice
    /// moderation.
    ///
    /// # The audio buffers
    ///
    /// Both callbacks receive `&mut [i16]` — interleaved 16-bit signed PCM,
    /// `AudioFrame::samples_per_channel * AudioFrame::channels` samples long. The
    /// samples may be modified **in place** to achieve simple DSP effects.
    ///
    /// The slice borrows memory owned by the SDK's voice engine and is valid
    /// **only for the duration of the callback**. It must not be stored; copy out
    /// of it if the data is needed later.
    ///
    /// # Muting handled audio
    ///
    /// Setting `should_mute` to `true` inside `received` causes that buffer to be
    /// muted after the callback returns. Use it when the application plays the
    /// incoming audio through its own audio engine and does not want Discord to
    /// play it as well.
    ///
    /// # Threading
    ///
    /// Unlike every other callback in this crate, these are driven by the voice
    /// engine rather than by [`run_callbacks`](crate::run_callbacks). They are on
    /// the real-time audio path: keep them short, and do not block, allocate
    /// heavily, or call back into the SDK from them.
    ///
    /// # Platform notes
    ///
    /// Same `Info.plist` requirements as [`start_call`](Self::start_call).
    pub fn start_call_with_audio_callbacks<R, C>(
        &mut self,
        lobby_id: u64,
        received: R,
        captured: C,
    ) -> Option<Call>
    where
        R: FnMut(u64, &mut [i16], AudioFrame, &mut bool) + 'static,
        C: FnMut(&mut [i16], AudioFrame) + 'static,
    {
        unsafe extern "C" fn received_tramp<R>(
            user_id: u64,
            data: *mut i16,
            samples_per_channel: u64,
            sample_rate: i32,
            channels: u64,
            out_should_mute: *mut bool,
            userdata: *mut c_void,
        ) where
            R: FnMut(u64, &mut [i16], AudioFrame, &mut bool) + 'static,
        {
            let frame = AudioFrame {
                samples_per_channel,
                sample_rate,
                channels,
            };
            // SAFETY: `userdata` is the boxed `R`. `data` points to `frame.len()`
            // interleaved samples the voice engine owns for the duration of this
            // call, and the SDK expects in-place edits, so a `&mut` slice is the
            // faithful model. A null or empty buffer degrades to an empty slice
            // rather than an invalid one. `out_should_mute` is written back only
            // when the SDK provided somewhere to write it.
            unsafe {
                callback::dispatch_mut::<R>(userdata, |f| {
                    let len = frame.sample_count();
                    let samples: &mut [i16] = if data.is_null() || len == 0 {
                        &mut []
                    } else {
                        std::slice::from_raw_parts_mut(data, len)
                    };
                    let mut should_mute = if out_should_mute.is_null() {
                        false
                    } else {
                        *out_should_mute
                    };
                    f(user_id, samples, frame, &mut should_mute);
                    if !out_should_mute.is_null() {
                        *out_should_mute = should_mute;
                    }
                })
            }
        }

        unsafe extern "C" fn captured_tramp<C>(
            data: *mut i16,
            samples_per_channel: u64,
            sample_rate: i32,
            channels: u64,
            userdata: *mut c_void,
        ) where
            C: FnMut(&mut [i16], AudioFrame) + 'static,
        {
            let frame = AudioFrame {
                samples_per_channel,
                sample_rate,
                channels,
            };
            // SAFETY: as for `received_tramp` — the buffer is valid for
            // `frame.len()` samples for the duration of this call and is intended
            // to be edited in place.
            unsafe {
                callback::dispatch_mut::<C>(userdata, |f| {
                    let len = frame.sample_count();
                    let samples: &mut [i16] = if data.is_null() || len == 0 {
                        &mut []
                    } else {
                        std::slice::from_raw_parts_mut(data, len)
                    };
                    f(samples, frame);
                })
            }
        }

        let mut raw = MaybeUninit::<sys::Discord_Call>::uninit();
        // SAFETY: the SDK owns both boxed closures and frees each via its own
        // `free_fn`. The out-parameter is only initialised — and only then owned
        // by us — when the call reports success.
        unsafe {
            let started = sys::Discord_Client_StartCallWithAudioCallbacks(
                self.as_raw_mut(),
                lobby_id,
                Some(received_tramp::<R>),
                callback::free_fn::<R>(),
                callback::persistent_userdata(received),
                Some(captured_tramp::<C>),
                callback::free_fn::<C>(),
                callback::persistent_userdata(captured),
                raw.as_mut_ptr(),
            );
            if started {
                Some(Call::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// End the call in `channel_id`, if there is one.
    ///
    /// Every [`Call`] handle for that channel is invalid once this completes and
    /// should be dropped.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn end_call<F>(&mut self, channel_id: u64, callback: F)
    where
        F: FnOnce() + 'static,
    {
        unsafe extern "C" fn tramp<F: FnOnce() + 'static>(userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_once::<F>(userdata, |f| f()) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_EndCall(
                self.as_raw_mut(),
                channel_id,
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// End every active call.
    ///
    /// Every [`Call`] handle is invalid once this completes and should be dropped.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn end_calls<F>(&mut self, callback: F)
    where
        F: FnOnce() + 'static,
    {
        unsafe extern "C" fn tramp<F: FnOnce() + 'static>(userdata: *mut c_void) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_once::<F>(userdata, |f| f()) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_EndCalls(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// The currently active call in `channel_id`, if any.
    pub fn call(&self, channel_id: u64) -> Option<Call> {
        let mut raw = MaybeUninit::<sys::Discord_Call>::uninit();
        // SAFETY: the out-parameter is only initialised — and only then owned by
        // us — when the getter returns `true`.
        unsafe {
            if sys::Discord_Client_GetCall(self.raw_ptr(), channel_id, raw.as_mut_ptr()) {
                Some(Call::from_raw(raw.assume_init()))
            } else {
                None
            }
        }
    }

    /// Every currently active call.
    pub fn calls(&self) -> Vec<Call> {
        // SAFETY: the getter fills the span and transfers both the array and its
        // elements to us.
        unsafe {
            span::out(
                |out| sys::Discord_Client_GetCalls(self.raw_ptr(), out),
                |raw| Call::from_raw(raw),
            )
        }
    }

    /// Be notified when a user in a lobby joins or leaves a voice call.
    ///
    /// The closure receives `(lobby_id, member_id, added)`, where `added` is
    /// `true` for a join.
    ///
    /// This exists so an application can show who is in voice in a lobby even
    /// when the current user has not joined and so has no [`Call`] to bind to.
    pub fn on_voice_participant_changed<F>(&mut self, callback: F)
    where
        F: FnMut(u64, u64, bool) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(u64, u64, bool) + 'static>(
            lobby_id: u64,
            member_id: u64,
            added: bool,
            userdata: *mut c_void,
        ) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(lobby_id, member_id, added)) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetVoiceParticipantChangedCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    // ---- Audio devices ----

    /// Asynchronously fetch the audio input device currently in use.
    ///
    /// [`None`] means the SDK reported no device.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn current_input_device<F>(&mut self, callback: F)
    where
        F: FnOnce(Option<AudioDevice>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetCurrentInputDevice(
                self.as_raw_mut(),
                Some(current_device_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Asynchronously fetch the audio output device currently in use.
    ///
    /// [`None`] means the SDK reported no device.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn current_output_device<F>(&mut self, callback: F)
    where
        F: FnOnce(Option<AudioDevice>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetCurrentOutputDevice(
                self.as_raw_mut(),
                Some(current_device_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Asynchronously fetch the audio input devices available to the user.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn input_devices<F>(&mut self, callback: F)
    where
        F: FnOnce(Vec<AudioDevice>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetInputDevices(
                self.as_raw_mut(),
                Some(device_list_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Asynchronously fetch the audio output devices available to the user.
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn output_devices<F>(&mut self, callback: F)
    where
        F: FnOnce(Vec<AudioDevice>) + 'static,
    {
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_GetOutputDevices(
                self.as_raw_mut(),
                Some(device_list_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Asynchronously switch the audio input device.
    ///
    /// `device_id` comes from [`AudioDevice::id`] on a device returned by
    /// [`input_devices`](Self::input_devices).
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn set_input_device<F>(&mut self, device_id: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the device id is copied by the SDK during the call; the boxed
        // closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_SetInputDevice(
                self.as_raw_mut(),
                string::borrow(device_id),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Asynchronously switch the audio output device.
    ///
    /// `device_id` comes from [`AudioDevice::id`] on a device returned by
    /// [`output_devices`](Self::output_devices).
    ///
    /// Completes through [`run_callbacks`](crate::run_callbacks).
    pub fn set_output_device<F>(&mut self, device_id: &str, callback: F)
    where
        F: FnOnce(Result<()>) + 'static,
    {
        // SAFETY: the device id is copied by the SDK during the call; the boxed
        // closure is owned by the SDK and freed via `free_fn`.
        unsafe {
            sys::Discord_Client_SetOutputDevice(
                self.as_raw_mut(),
                string::borrow(device_id),
                Some(result_tramp::<F>),
                callback::free_fn::<Option<F>>(),
                callback::once_userdata(callback),
            )
        }
    }

    /// Be notified when Discord detects a change in the available audio devices.
    ///
    /// The closure receives the full input and output device lists as owned
    /// values, so it may keep them past the callback.
    pub fn on_device_change<F>(&mut self, callback: F)
    where
        F: FnMut(Vec<AudioDevice>, Vec<AudioDevice>) + 'static,
    {
        unsafe extern "C" fn tramp<F>(
            input_devices: sys::Discord_AudioDeviceSpan,
            output_devices: sys::Discord_AudioDeviceSpan,
            userdata: *mut c_void,
        ) where
            F: FnMut(Vec<AudioDevice>, Vec<AudioDevice>) + 'static,
        {
            // SAFETY: `userdata` is the boxed `F`. Both spans are handed over to
            // the callback and adopted exactly once here.
            unsafe {
                let inputs = take_device_span(input_devices);
                let outputs = take_device_span(output_devices);
                callback::dispatch_mut::<F>(userdata, move |f| f(inputs, outputs))
            }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetDeviceChangeCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }

    /// Show the system audio route picker.
    ///
    /// iOS only; returns whether the picker was shown.
    pub fn show_audio_route_picker(&mut self) -> bool {
        // SAFETY: only requires an initialised handle.
        unsafe { sys::Discord_Client_ShowAudioRoutePicker(self.as_raw_mut()) }
    }

    // ---- Volume ----

    /// The current user's microphone volume.
    ///
    /// A percentage in the range `[0, 100]` representing perceptual loudness.
    pub fn input_volume(&self) -> f32 {
        // SAFETY: a read-only getter on an initialised handle.
        unsafe { sys::Discord_Client_GetInputVolume(self.raw_ptr()) }
    }

    /// Set the current user's microphone volume.
    ///
    /// A percentage in the range `[0, 100]` representing perceptual loudness.
    pub fn set_input_volume(&mut self, input_volume: f32) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetInputVolume(self.as_raw_mut(), input_volume) }
    }

    /// The current user's speaker volume.
    ///
    /// A percentage in the range `[0, 200]` representing perceptual loudness.
    pub fn output_volume(&self) -> f32 {
        // SAFETY: a read-only getter on an initialised handle.
        unsafe { sys::Discord_Client_GetOutputVolume(self.raw_ptr()) }
    }

    /// Set the current user's speaker volume.
    ///
    /// A percentage in the range `[0, 200]` representing perceptual loudness.
    pub fn set_output_volume(&mut self, output_volume: f32) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetOutputVolume(self.as_raw_mut(), output_volume) }
    }

    // ---- Mute and deafen, across every call ----

    /// Whether the current user is deafened in all calls.
    pub fn self_deaf_all(&self) -> bool {
        // SAFETY: a read-only getter on an initialised handle.
        unsafe { sys::Discord_Client_GetSelfDeafAll(self.raw_ptr()) }
    }

    /// Deafen or undeafen the current user in every call.
    ///
    /// A deafened user hears no other participant, and no other participant
    /// hears them.
    ///
    /// This overrides the per-call
    /// [`Call::set_self_deaf`](crate::call::Call::set_self_deaf) setting.
    pub fn set_self_deaf_all(&mut self, deaf: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetSelfDeafAll(self.as_raw_mut(), deaf) }
    }

    /// Whether the current user's microphone is muted in all calls.
    pub fn self_mute_all(&self) -> bool {
        // SAFETY: a read-only getter on an initialised handle.
        unsafe { sys::Discord_Client_GetSelfMuteAll(self.raw_ptr()) }
    }

    /// Mute or unmute the current user's microphone in every call.
    ///
    /// No other participant in any active call can hear a muted user.
    ///
    /// This overrides the per-call
    /// [`Call::set_self_mute`](crate::call::Call::set_self_mute) setting.
    pub fn set_self_mute_all(&mut self, mute: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetSelfMuteAll(self.as_raw_mut(), mute) }
    }

    // ---- Voice processing ----
    //
    // These all default to sensible values. They exist for applications that
    // expose a voice-settings UI of their own, mirroring Discord's.

    /// Enable or disable the basic echo cancellation provided by WebRTC.
    ///
    /// Defaults to on. Generally not needed unless building a voice settings UI.
    pub fn set_echo_cancellation(&mut self, on: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetEchoCancellation(self.as_raw_mut(), on) }
    }

    /// Enable or disable basic background noise suppression.
    ///
    /// Defaults to on. Generally not needed unless building a voice settings UI.
    pub fn set_noise_suppression(&mut self, on: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetNoiseSuppression(self.as_raw_mut(), on) }
    }

    /// Enable or disable Krisp noise cancellation.
    ///
    /// Defaults to off. Enabling it automatically disables
    /// [`set_noise_suppression`](Self::set_noise_suppression).
    pub fn set_noise_cancellation(&mut self, on: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetNoiseCancellation(self.as_raw_mut(), on) }
    }

    /// Enable or disable automatic microphone gain adjustment.
    ///
    /// When on, the microphone volume is adjusted automatically to stay clear and
    /// consistent. Defaults to on. Generally not needed unless building a voice
    /// settings UI.
    pub fn set_automatic_gain_control(&mut self, on: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetAutomaticGainControl(self.as_raw_mut(), on) }
    }

    /// Enable or disable hardware encoding and decoding for audio, where available.
    ///
    /// Defaults to on for both. This must be called immediately after
    /// constructing the [`Client`]; if called too late the SDK logs an error and
    /// the setting does not take effect.
    pub fn set_opus_hardware_coding(&mut self, encode: bool, decode: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetOpusHardwareCoding(self.as_raw_mut(), encode, decode) }
    }

    /// Enable or disable AEC diagnostic recording.
    ///
    /// Used to diagnose acoustic echo cancellation problems: the input and output
    /// waveform data is written to the log directory.
    pub fn set_aec_dump(&mut self, on: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetAecDump(self.as_raw_mut(), on) }
    }

    /// On mobile, choose whether the game engine or the SDK owns the audio session.
    ///
    /// This covers `AudioManager` state on Android and `AVAudioSession`
    /// activation on iOS. It must be called **before connecting to any call** if
    /// the application manages audio itself; otherwise the voice engine ends
    /// audio management when the last call ends.
    ///
    /// The Unity plugin calls this automatically when the native Unity audio
    /// engine is enabled in the project settings.
    pub fn set_engine_managed_audio_session(&mut self, is_engine_managed: bool) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetEngineManagedAudioSession(self.as_raw_mut(), is_engine_managed) }
    }

    /// On mobile devices, enable speakerphone mode.
    ///
    /// Returns whether the mode was applied.
    #[deprecated(note = "Discord deprecated Client::SetSpeakerMode; it has no replacement.")]
    pub fn set_speaker_mode(&mut self, speaker_mode: bool) -> bool {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetSpeakerMode(self.as_raw_mut(), speaker_mode) }
    }

    // ---- No-audio-input detection ----

    /// Set the threshold below which the microphone counts as receiving no audio.
    ///
    /// Specified in dBFS (decibels relative to full scale), range
    /// `[-100.0, 100.0]`. Defaults to `-100.0`, which disables the feature.
    ///
    /// Useful for catching a misconfigured microphone: with push to talk, the
    /// user may press their talk key while no audio is actually arriving. Pair
    /// this with [`on_no_audio_input`](Self::on_no_audio_input) to notice and
    /// tell them.
    pub fn set_no_audio_input_threshold(&mut self, dbfs_threshold: f32) {
        // SAFETY: a plain setter on an initialised handle.
        unsafe { sys::Discord_Client_SetNoAudioInputThreshold(self.as_raw_mut(), dbfs_threshold) }
    }

    /// Be notified when microphone input starts or stops being detected.
    ///
    /// Only fires once a threshold has been set with
    /// [`set_no_audio_input_threshold`](Self::set_no_audio_input_threshold). The
    /// closure receives `true` when input is being detected.
    pub fn on_no_audio_input<F>(&mut self, callback: F)
    where
        F: FnMut(bool) + 'static,
    {
        unsafe extern "C" fn tramp<F: FnMut(bool) + 'static>(
            input_detected: bool,
            userdata: *mut c_void,
        ) {
            // SAFETY: `userdata` is the boxed `F` installed below.
            unsafe { callback::dispatch_mut::<F>(userdata, |f| f(input_detected)) }
        }
        // SAFETY: the SDK owns the boxed closure and frees it via `free_fn`.
        unsafe {
            sys::Discord_Client_SetNoAudioInputCallback(
                self.as_raw_mut(),
                Some(tramp::<F>),
                callback::free_fn::<F>(),
                callback::persistent_userdata(callback),
            )
        }
    }
}
