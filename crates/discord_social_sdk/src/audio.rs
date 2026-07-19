//! Audio devices and voice activity detection settings.
//!
//! Discord initialises its audio engine with the system default input and
//! output devices. [`AudioDevice`] describes one such device — the `Client`
//! enumerates them and accepts an [`AudioDevice::id`] to switch to a different
//! one.
//!
//! [`VadThresholdSettings`] describes how sensitive voice auto detection is when
//! deciding whether the user's microphone is picking up speech. It is read back
//! from a call with [`Call::vad_threshold`](crate::call::Call::vad_threshold)
//! and configured with
//! [`Call::set_vad_threshold`](crate::call::Call::set_vad_threshold).

use discord_social_sdk_sys as sys;

use crate::handle::handle;
use crate::string;

handle! {
    /// A single input or output audio device available to the user.
    ///
    /// Discord initialises the audio engine with the system default input and
    /// output devices. You can change the device through the `Client` by passing
    /// the [`id`](AudioDevice::id) of the desired audio device.
    ///
    /// The SDK exposes no constructor for this type, so instances only arrive
    /// from the `Client`'s device enumeration callbacks.
    AudioDevice(sys::Discord_AudioDevice) {
        drop: sys::Discord_AudioDevice_Drop,
        clone: sys::Discord_AudioDevice_Clone,
    }
}

impl AudioDevice {
    /// The ID of the audio device.
    ///
    /// This is the value to hand back to the `Client` when selecting a device.
    pub fn id(&self) -> String {
        // SAFETY: the getter fills the out-parameter and transfers the buffer to us.
        unsafe { string::out(|out| sys::Discord_AudioDevice_Id(self.raw_ptr(), out)) }
    }

    /// Set the ID of the audio device.
    pub fn set_id(&mut self, value: &str) {
        // SAFETY: the SDK copies the string before returning, so borrowing is sound.
        unsafe { sys::Discord_AudioDevice_SetId(self.as_raw_mut(), string::borrow(value)) }
    }

    /// The display name of the audio device.
    pub fn name(&self) -> String {
        // SAFETY: the getter fills the out-parameter and transfers the buffer to us.
        unsafe { string::out(|out| sys::Discord_AudioDevice_Name(self.raw_ptr(), out)) }
    }

    /// Set the display name of the audio device.
    pub fn set_name(&mut self, value: &str) {
        // SAFETY: the SDK copies the string before returning, so borrowing is sound.
        unsafe { sys::Discord_AudioDevice_SetName(self.as_raw_mut(), string::borrow(value)) }
    }

    /// Whether the audio device is the system default device.
    pub fn is_default(&self) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_AudioDevice_IsDefault(self.raw_ptr()) }
    }

    /// Set whether the audio device is the system default device.
    pub fn set_is_default(&mut self, value: bool) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_AudioDevice_SetIsDefault(self.as_raw_mut(), value) }
    }
}

/// Compares the ID of two audio devices for equality.
impl PartialEq for AudioDevice {
    fn eq(&self, other: &Self) -> bool {
        // SAFETY: both handles are live and the SDK only reads through them.
        unsafe { sys::Discord_AudioDevice_Equals(self.raw_ptr(), other.as_raw()) }
    }
}

impl Eq for AudioDevice {}

impl std::fmt::Debug for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDevice")
            .field("id", &self.id())
            .field("name", &self.name())
            .field("is_default", &self.is_default())
            .finish()
    }
}

handle! {
    /// Settings for the voice auto detection threshold used to pick up activity
    /// from a user's microphone.
    ///
    /// Obtained from [`Call::vad_threshold`](crate::call::Call::vad_threshold).
    /// The SDK exposes neither a constructor nor a clone for this type; to
    /// change a call's configuration use
    /// [`Call::set_vad_threshold`](crate::call::Call::set_vad_threshold) rather
    /// than mutating a settings object read back from a call.
    VadThresholdSettings(sys::Discord_VADThresholdSettings) {
        drop: sys::Discord_VADThresholdSettings_Drop,
    }
}

impl VadThresholdSettings {
    /// The current voice auto detection threshold value.
    ///
    /// Has a range of `-100` to `0` and defaults to `-60`.
    pub fn vad_threshold(&self) -> f32 {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_VADThresholdSettings_VadThreshold(self.raw_ptr()) }
    }

    /// Set the voice auto detection threshold value.
    ///
    /// Has a range of `-100` to `0` and defaults to `-60`.
    pub fn set_vad_threshold(&mut self, value: f32) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_VADThresholdSettings_SetVadThreshold(self.as_raw_mut(), value) }
    }

    /// Whether Discord is currently automatically setting and detecting the
    /// appropriate threshold to use.
    pub fn automatic(&self) -> bool {
        // SAFETY: a read-only getter on a live handle.
        unsafe { sys::Discord_VADThresholdSettings_Automatic(self.raw_ptr()) }
    }

    /// Set whether Discord automatically detects the appropriate threshold.
    pub fn set_automatic(&mut self, value: bool) {
        // SAFETY: a plain setter on a live handle.
        unsafe { sys::Discord_VADThresholdSettings_SetAutomatic(self.as_raw_mut(), value) }
    }
}

impl std::fmt::Debug for VadThresholdSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VadThresholdSettings")
            .field("automatic", &self.automatic())
            .field("vad_threshold", &self.vad_threshold())
            .finish()
    }
}
