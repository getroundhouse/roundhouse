//! A recording quiets what's playing, in each way it's asked to, from when it's asked until it's stopped, so what's
//! playing is heard again while its last words go in. These recordings are fed, so they say whether they'd quiet it
//! rather than quieting it.

use sound::Quieting;

use super::support::{SCRIPT, dictation};
use crate::fake::FakeEngine;
use crate::{AudioIn, Hints};

#[test]
fn a_recording_quiets_each_way_asked_until_it_is_stopped() {
    let dictation = dictation(FakeEngine::new(SCRIPT));
    let (recording, _updates) = dictation.record(AudioIn::Fed, Hints::default(), crate::Language::Auto);
    assert!(!recording.quiets(Quieting::MuteOutput), "nothing is quieted until it's asked");

    recording.quiet(Quieting::MuteOutput);
    assert!(recording.quiets(Quieting::MuteOutput));
    assert!(!recording.quiets(Quieting::PauseMedia), "only the way asked");
    recording.quiet(Quieting::PauseMedia);
    assert!(Quieting::ALL.into_iter().all(|how| recording.quiets(how)));
    recording.stop();

    assert!(Quieting::ALL.into_iter().all(|how| !recording.quiets(how)));
}

#[test]
fn a_stopped_recording_quiets_nothing() {
    let dictation = dictation(FakeEngine::new(SCRIPT));
    let (recording, _updates) = dictation.record(AudioIn::Fed, Hints::default(), crate::Language::Auto);
    recording.stop();

    for how in Quieting::ALL {
        recording.quiet(how);
    }

    assert!(Quieting::ALL.into_iter().all(|how| !recording.quiets(how)));
}
