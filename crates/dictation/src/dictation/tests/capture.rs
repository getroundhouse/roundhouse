//! A capture resource must be dropped while the model is still held at a gate, not once recognition resumes.

use std::sync::Arc;

use super::support::{KEEP, SCRIPT, TIMEOUT, final_text, the_rest};
use super::watched::{Gate, Watch, Watched};
use crate::dictation::buffer::{Buffer, MAX_SAMPLES};
use crate::dictation::capture::{Capture, Open};
use crate::dictation::recording::Input;
use crate::fake::FakeEngine;
use crate::{AudioIn, Dictation, DictationError, Ending, Engine, EngineError, Update};

struct Microphone(flume::Sender<()>);

impl Drop for Microphone {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn capture() -> (Open, flume::Receiver<Buffer>, flume::Receiver<()>) {
    let (started, audio) = flume::bounded(1);
    let (closed, gone) = flume::bounded(1);
    let open: Open = Arc::new(move |buffer| {
        started.send(buffer).unwrap();
        Ok(Box::new(Microphone(closed.clone())))
    });
    (open, audio, gone)
}

#[test]
fn stop_and_drop_release_capture_while_the_model_is_loading() {
    for stop in [true, false] {
        let engine = FakeEngine::new(SCRIPT);
        let words = engine.saying("fix the test");
        let (gate, reached, release) = Gate::new();
        let (capture, audio, gone) = capture();
        let dictation = Dictation::with_capture(
            move || {
                gate.pass();
                Ok(Box::new(engine.clone()) as Box<dyn Engine>)
            },
            KEEP,
            capture,
        )
        .unwrap();
        let (recording, updates) = dictation.record(AudioIn::Microphone);
        let audio = audio.recv_timeout(TIMEOUT).unwrap();
        reached.recv_timeout(TIMEOUT).unwrap();
        audio.audio(words.clone());

        if stop {
            recording.stop();
        } else {
            drop(recording);
        }

        gone.recv_timeout(TIMEOUT).expect("capture ends before loading resumes");
        audio.audio(words);
        drop(release);
        let updates = the_rest(&updates);
        if stop {
            assert_eq!(
                updates.iter().filter(|u| matches!(u, Update::Final(_))).collect::<Vec<_>>(),
                [&final_text("Fix the test.")]
            );
            assert_eq!(updates.last(), Some(&Update::Ended(Ending::Heard)));
        } else {
            assert!(updates.is_empty(), "cancelled audio is not recognized: {updates:?}");
        }
    }
}

#[test]
fn stop_and_cancel_release_capture_while_inference_is_blocked() {
    for stop in [true, false] {
        let engine = FakeEngine::new(SCRIPT);
        let words = engine.saying("fix the test. / run it");
        let (gate, reached, release) = Gate::new();
        let (watch, _) = Watch::new();
        let (capture, audio, gone) = capture();
        let dictation = Dictation::with_capture(
            move || {
                Ok(Box::new(Watched { engine: engine.clone(), watch: watch.clone(), gate: Some(gate.clone()) })
                    as Box<dyn Engine>)
            },
            KEEP,
            capture,
        )
        .unwrap();
        let (recording, updates) = dictation.record(AudioIn::Microphone);
        let audio = audio.recv_timeout(TIMEOUT).unwrap();
        audio.audio(words);
        reached.recv_timeout(TIMEOUT).unwrap();

        if stop {
            recording.stop();
        } else {
            recording.cancel();
        }

        gone.recv_timeout(TIMEOUT).expect("capture ends before inference resumes");
        drop(release);
        let updates = the_rest(&updates);
        if stop {
            assert_eq!(updates.last(), Some(&Update::Ended(Ending::Heard)));
            assert_eq!(
                updates.iter().filter(|u| matches!(u, Update::Final(_))).collect::<Vec<_>>(),
                [&final_text("Fix the test."), &final_text("Run it.")]
            );
        } else {
            assert!(!updates.iter().any(|u| matches!(u, Update::Final(_) | Update::Ended(_))), "{updates:?}");
        }
    }
}

#[test]
fn overflow_and_device_failure_release_capture_while_the_model_is_blocked() {
    for overflow in [true, false] {
        let (gate, reached, release) = Gate::new();
        let (capture, audio, gone) = capture();
        let dictation = Dictation::with_capture(
            move || {
                gate.pass();
                Ok(Box::new(FakeEngine::new(SCRIPT)) as Box<dyn Engine>)
            },
            KEEP,
            capture,
        )
        .unwrap();
        let (_recording, updates) = dictation.record(AudioIn::Microphone);
        let audio = audio.recv_timeout(TIMEOUT).unwrap();
        reached.recv_timeout(TIMEOUT).unwrap();

        let expected = if overflow {
            audio.audio(vec![0.; MAX_SAMPLES]);
            audio.audio(vec![0.1]);
            DictationError::AudioBacklog
        } else {
            audio.end(Input::Failed(DictationError::NoMicrophone));
            DictationError::NoMicrophone
        };

        gone.recv_timeout(TIMEOUT).expect("failed capture doesn't wait for the model");
        drop(release);
        assert_eq!(the_rest(&updates), [Update::Failed(expected)]);
    }
}

#[test]
fn a_model_error_releases_capture_without_the_recording_handle_being_dropped() {
    for loading in [true, false] {
        let engine = FakeEngine::new(SCRIPT).failing();
        let words = engine.saying("fix the test. / run it");
        let (capture, audio, gone) = capture();
        let dictation = Dictation::with_capture(
            move || {
                if loading {
                    Err(EngineError("failed to load".to_string()))
                } else {
                    Ok(Box::new(engine.clone()) as Box<dyn Engine>)
                }
            },
            KEEP,
            capture,
        )
        .unwrap();
        let (_recording, updates) = dictation.record(AudioIn::Microphone);
        let audio = audio.recv_timeout(TIMEOUT).unwrap();

        audio.audio(words);

        assert!(matches!(the_rest(&updates).last(), Some(Update::Failed(DictationError::Engine(_)))));
        gone.recv_timeout(TIMEOUT).expect("a failed model releases its microphone");
        assert!(!audio.accepting());
    }
}

#[test]
fn a_recording_ended_before_capture_starts_never_opens_the_microphone() {
    for ending in [Input::Stop, Input::Cancel] {
        let buffer = Buffer::new();
        buffer.end(ending);

        let _capture =
            Capture::start(buffer, Arc::new(|_| panic!("an ended recording must not open capture"))).unwrap();
    }
}
