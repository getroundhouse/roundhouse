//! A recording mutes the sound output from when it's asked to until it's stopped, so what's playing is heard again
//! while its last words go in. These recordings are fed, so they say whether they'd mute rather than muting.

use super::support::{SCRIPT, dictation};
use crate::fake::FakeEngine;
use crate::{AudioIn, Hints};

#[test]
fn a_recording_mutes_the_output_until_it_is_stopped() {
    let dictation = dictation(FakeEngine::new(SCRIPT));
    let (recording, _updates) = dictation.record(AudioIn::Fed, Hints::default(), crate::Language::Auto);
    assert!(!recording.mutes_output(), "nothing is muted until it's asked");

    recording.mute_output();
    assert!(recording.mutes_output());
    recording.stop();

    assert!(!recording.mutes_output());
}

#[test]
fn a_stopped_recording_mutes_nothing() {
    let dictation = dictation(FakeEngine::new(SCRIPT));
    let (recording, _updates) = dictation.record(AudioIn::Fed, Hints::default(), crate::Language::Auto);
    recording.stop();

    recording.mute_output();

    assert!(!recording.mutes_output());
}
