//! Core Audio's calls that mute and unmute the device a Mac's sound plays from. Each waits on `coreaudiod`, another
//! process, so only the thread in `muting` makes them.

use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertySelector,
    AudioObjectSetPropertyData, kAudioDevicePropertyMute, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput,
    kAudioObjectSystemObject, kAudioObjectUnknown,
};

/// Mutes the device sound plays from now, and says which it is. `None` where nothing was muted, so nothing is to be
/// unmuted after: there's no output device, it's muted already, which it stays, or it can't be muted, as a display's
/// speakers often can't, whose volume only the display sets.
pub(super) fn mute_default() -> Option<AudioObjectID> {
    let system = AudioObjectID::try_from(kAudioObjectSystemObject).ok()?;
    let device = read(system, address(kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal))?;
    if device == kAudioObjectUnknown || read(device, mute())? != 0 {
        return None;
    }
    set_muted(device, true).then_some(device)
}

/// Unmutes `device`, which [`mute_default`] muted, even once sound plays from another, such as headphones plugged in
/// meanwhile, so it's left as it was. One the person unmuted meanwhile stays so.
pub(super) fn unmute(device: AudioObjectID) {
    // A device that's gone, such as headphones unplugged, can't be unmuted, and needs no more.
    let _ = set_muted(device, false);
}

/// Whether a device is muted, read and set as a `u32` that's 0 or 1, on the device as a whole rather than one of its
/// channels.
fn mute() -> AudioObjectPropertyAddress {
    address(kAudioDevicePropertyMute, kAudioObjectPropertyScopeOutput)
}

fn address(selector: AudioObjectPropertySelector, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain }
}

/// The `u32` that `object` has at `address`, which both properties read here are: a device's ID and whether it's
/// muted. `None` when it has no such property, or gives something else.
fn read(object: AudioObjectID, address: AudioObjectPropertyAddress) -> Option<u32> {
    let mut value = 0_u32;
    let mut size = SIZE;
    // SAFETY: `value` is as big as `size` says, and Core Audio writes no more than that into it, and the address and the
    // size are read and written only during the call.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast::<c_void>(),
        )
    };
    (status == 0 && size == SIZE).then_some(value)
}

/// Whether Core Audio muted or unmuted `device`, as `muted` says.
fn set_muted(device: AudioObjectID, muted: bool) -> bool {
    let value = u32::from(muted);
    // SAFETY: `value` is as big as `SIZE` says, and Core Audio only reads it, and the address, during the call.
    let status = unsafe {
        AudioObjectSetPropertyData(
            device,
            NonNull::from(&mute()),
            0,
            ptr::null(),
            SIZE,
            NonNull::from(&value).cast::<c_void>(),
        )
    };
    status == 0
}

/// The size of a `u32`, in bytes, as Core Audio is given it.
const SIZE: u32 = u32::BITS / 8;
