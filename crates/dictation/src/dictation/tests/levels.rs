//! Loudness reaches a field independently of a recognizer that is still loading.

use super::support::{KEEP, TIMEOUT};
use super::watched::Gate;
use crate::fake::FakeEngine;
use crate::{AudioIn, Dictation, Hints};

#[test]
fn audio_levels_arrive_while_the_model_is_blocked() {
    let (gate, reached, open) = Gate::new();
    let dictation = Dictation::start(
        move || {
            gate.pass();
            Ok(Box::new(FakeEngine::new(&[])))
        },
        KEEP,
    )
    .unwrap();
    let (recording, _) = dictation.record(AudioIn::Fed, Hints::default(), crate::Language::Auto);
    let levels = recording.levels();
    reached.recv_timeout(TIMEOUT).unwrap();

    recording.feed(vec![0.1; 480]);
    assert!(levels.try_recv().unwrap().strength() > 0.5, "capture doesn't wait for recognition");
    recording.feed(vec![0.; 480]);
    assert_eq!(levels.try_recv().unwrap().strength(), 0.);

    recording.cancel();
    open.send(()).unwrap();
}
