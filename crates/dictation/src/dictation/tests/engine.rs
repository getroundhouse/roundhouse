//! Loading the engine, letting go of it, and its failing.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::support::{KEEP, SCRIPT, TIMEOUT, dictation, feed, the_rest};
use super::watched::{Watch, Watched};
use crate::dictation::{Dictation, DictationError, Ending, Update};
use crate::fake::FakeEngine;
use crate::{AudioIn, Engine, EngineError};

#[test]
fn a_failing_engine_ends_the_recording_with_its_error() {
    let engine = FakeEngine::new(SCRIPT).failing();
    let audio = engine.saying("fix the test");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed);

    feed(&recording, &audio);
    recording.stop();

    let last = the_rest(&updates).pop();
    assert!(matches!(last, Some(Update::Failed(DictationError::Engine(_)))), "{last:?}");
}

#[test]
fn the_engine_loads_once_for_every_recording_and_preparing_loads_it_ahead() {
    let loads = Arc::new(AtomicUsize::new(0));
    let counted = loads.clone();
    let engine = FakeEngine::new(SCRIPT);
    let audio = engine.saying("fix it");
    let dictation = Dictation::start(
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(engine.clone()) as Box<dyn Engine>)
        },
        KEEP,
    )
    .unwrap();

    assert_eq!(dictation.prepare().recv_timeout(TIMEOUT).unwrap(), Ok(()));
    for _ in 0..2 {
        let (recording, updates) = dictation.record(AudioIn::Fed);
        feed(&recording, &audio);
        recording.stop();
        assert_eq!(the_rest(&updates).last(), Some(&Update::Ended(Ending::Heard)));
    }

    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[test]
fn an_engine_unused_for_a_while_is_let_go_and_loaded_again_when_needed() {
    let loads = Arc::new(AtomicUsize::new(0));
    let ((watch, let_go), counted) = (Watch::new(), loads.clone());
    let load = move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Watched { engine: FakeEngine::new(SCRIPT), watch: watch.clone(), gate: None }) as Box<dyn Engine>)
    };
    let dictation = Dictation::start(load, Duration::from_millis(50)).unwrap();
    dictation.prepare().recv_timeout(TIMEOUT).unwrap().unwrap();

    let_go.recv_timeout(TIMEOUT).expect("let go of once unused");
    dictation.prepare().recv_timeout(TIMEOUT).unwrap().unwrap();

    assert_eq!(loads.load(Ordering::SeqCst), 2);
}

#[test]
fn an_engine_that_fails_to_load_fails_the_recording_and_is_tried_again_next_time() {
    let tries = Arc::new(AtomicUsize::new(0));
    let counted = tries.clone();
    let dictation = Dictation::start(
        move || match counted.fetch_add(1, Ordering::SeqCst) {
            0 => Err(EngineError("not downloaded".to_string())),
            _ => Ok(Box::new(FakeEngine::new(SCRIPT)) as Box<dyn Engine>),
        },
        KEEP,
    )
    .unwrap();

    let (_recording, updates) = dictation.record(AudioIn::Fed);
    let failed = the_rest(&updates);

    assert_eq!(failed, [Update::Failed(DictationError::Engine(EngineError("not downloaded".to_string())))]);
    assert_eq!(dictation.prepare().recv_timeout(TIMEOUT).unwrap(), Ok(()));
}
