//! Nothing more said of a recording once it's cancelled, whatever the thread was doing.

use flume::RecvTimeoutError;

use super::support::{KEEP, SCRIPT, TIMEOUT, dictation, feed, final_text, the_rest, until};
use super::watched::{Gate, Watch, Watched};
use crate::dictation::{Dictation, Update};
use crate::fake::FakeEngine;
use crate::{AudioIn, Engine, EngineError};

#[test]
fn a_cancelled_recording_says_nothing_more() {
    let engine = FakeEngine::new(SCRIPT);
    let audio = engine.saying("fix the test. / run it");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed);
    feed(&recording, &audio);
    until(&updates, &final_text("Fix the test."));

    recording.cancel();

    let rest = the_rest(&updates);
    assert!(!rest.iter().any(|u| matches!(u, Update::Final(_) | Update::Ended(_))), "{rest:?}");
    assert_eq!(updates.recv_timeout(TIMEOUT), Err(RecvTimeoutError::Disconnected), "the recording's channel closes");
}

#[test]
fn a_recording_cancelled_while_the_engine_loads_is_not_heard_and_says_nothing_of_how_loading_went() {
    for loads in [true, false] {
        let engine = FakeEngine::new(SCRIPT);
        let audio = engine.saying("fix the test. / run it");
        let ((gate, at_gate, open), (watch, _)) = (Gate::new(), Watch::new());
        let watched = watch.clone();
        let load = move || {
            gate.pass();
            if !loads {
                return Err(EngineError("not downloaded".to_string()));
            }
            Ok(Box::new(Watched { engine: engine.clone(), watch: watched.clone(), gate: None }) as Box<dyn Engine>)
        };
        let dictation = Dictation::start(load, KEEP).unwrap();
        let (recording, updates) = dictation.record(AudioIn::Fed);
        at_gate.recv_timeout(TIMEOUT).unwrap();
        feed(&recording, &audio);

        recording.cancel();
        open.send(()).unwrap();

        assert_eq!(updates.recv_timeout(TIMEOUT), Err(RecvTimeoutError::Disconnected), "loads: {loads}");
        assert_eq!(watch.listened(), 0, "not even started on");
    }
}

#[test]
fn a_sentence_being_made_final_as_its_recording_is_cancelled_says_nothing_and_nothing_after_it_is_heard() {
    let engine = FakeEngine::new(SCRIPT);
    let audio = engine.saying("fix the test. / run it");
    let ((gate, at_gate, open), (watch, _)) = (Gate::new(), Watch::new());
    let watched = watch.clone();
    let load = move || {
        let gate = Some(gate.clone());
        Ok(Box::new(Watched { engine: engine.clone(), watch: watched.clone(), gate }) as Box<dyn Engine>)
    };
    let dictation = Dictation::start(load, KEEP).unwrap();
    let (recording, updates) = dictation.record(AudioIn::Fed);
    feed(&recording, &audio);
    at_gate.recv_timeout(TIMEOUT).unwrap();
    let heard = watch.heard();

    recording.cancel();
    open.send(()).unwrap();

    let rest = the_rest(&updates);
    assert!(!rest.iter().any(|u| matches!(u, Update::Final(_) | Update::Ended(_))), "{rest:?}");
    assert_eq!(updates.recv_timeout(TIMEOUT), Err(RecvTimeoutError::Disconnected));
    assert!(heard < audio.len(), "what follows the first piece is still to be heard");
    assert_eq!(watch.heard(), heard, "what was still to be heard goes unheard");
}
