//! Muting on a thread of its own, as `audio::changes` makes each change, since each of Core Audio's calls waits on
//! `coreaudiod`, which can be slow or wedged, such as while it restarts.

use objc2_core_audio::AudioObjectID;

use super::device;
use crate::audio::changes::{Changes, Held};

/// Whether this platform can mute its sound output while it's dictated to. A Mac can.
pub(crate) const MUTES_OUTPUT: bool = true;

/// The sound output [`mute_output`] asked to have muted, unmuted once this is dropped, if it was muted.
pub(crate) type MutedOutput = Held;

static MUTING: Changes<AudioObjectID> = Changes::new("dictation-sound-output", device::mute_default, device::unmute);

/// Has the device sound plays from muted, without waiting for it. `None` where the thread couldn't start, so nothing
/// will be. Otherwise the thread mutes it unless it's muted already or can't be muted, as [`device::mute_default`]
/// says.
pub(crate) fn mute_output() -> Option<MutedOutput> {
    MUTING.hold()
}

/// Answers once everything muted before it, and let go of, is unmuted, as [`Changes::restored`] says.
pub(crate) fn output_restored() -> flume::Receiver<()> {
    MUTING.restored()
}
