//! The copy of a recording's audio is all that came, however the recording ended and whatever the engine got to.

use super::support::{KEEP, SCRIPT, TIMEOUT, dictation, feed, the_rest};
use super::watched::Gate;
use crate::dictation::buffer::{Buffer, MAX_SAMPLES};
use crate::dictation::{Copied, Dictation};
use crate::fake::FakeEngine;
use crate::{AudioIn, Engine, Hints, Language};

/// Everything copied, once the last of it has come.
fn all(audio: &flume::Receiver<Vec<f32>>) -> Vec<f32> {
    let mut all = Vec::new();
    while let Ok(buffer) = audio.recv_timeout(TIMEOUT) {
        all.extend(buffer);
    }
    assert!(audio.is_disconnected(), "it closes once the last has come");
    all
}

#[test]
fn a_recording_that_stops_is_copied_whole() {
    let engine = FakeEngine::new(SCRIPT);
    let said = engine.saying("fix the test. / run it");
    let Copied { recording, updates, audio } =
        dictation(engine).record_copying(AudioIn::Fed, Hints::default(), Language::Auto);
    feed(&recording, &said);
    recording.stop();
    recording.feed(vec![0.5; 160]);

    the_rest(&updates);
    assert_eq!(all(&audio), said, "and nothing after it stopped");
}

#[test]
fn one_cancelled_before_the_engine_got_to_it_is_copied_whole() {
    let engine = FakeEngine::new(SCRIPT);
    let said = engine.saying("fix the test. / run it");
    let (gate, at_gate, open) = Gate::new();
    let load = move || {
        gate.pass();
        Ok(Box::new(engine.clone()) as Box<dyn Engine>)
    };
    let dictation = Dictation::start(load, KEEP).unwrap();
    let Copied { recording, audio, .. } = dictation.record_copying(AudioIn::Fed, Hints::default(), Language::Auto);
    at_gate.recv_timeout(TIMEOUT).unwrap();
    feed(&recording, &said);
    recording.cancel();

    assert_eq!(all(&audio), said);
    drop(open);
}

#[test]
fn audio_that_comes_faster_than_its_heard_is_copied_with_the_buffer_that_was_too_much() {
    let (copy, audio) = flume::unbounded();
    let buffer = Buffer::new(Some(copy));
    buffer.audio(vec![0.1; MAX_SAMPLES]);
    buffer.audio(vec![0.2]);

    assert_eq!(all(&audio).len(), MAX_SAMPLES + 1);
}
