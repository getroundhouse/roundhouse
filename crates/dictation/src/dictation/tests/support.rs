//! What every test of the dictation thread uses: a fake engine on it, and reading what's heard.

use std::time::Duration;

use crate::Engine;
use crate::dictation::{Dictation, Recording, Update};
use crate::fake::FakeEngine;

/// Long enough for anything a test waits on, which takes milliseconds.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

/// Longer than any test, so the engine is kept for all of it.
pub(super) const KEEP: Duration = Duration::from_secs(600);

pub(super) const SCRIPT: &[&str] = &["fix", "the", "flaky", "test", "run", "it", "again", "please"];

pub(super) fn dictation(engine: FakeEngine) -> Dictation {
    Dictation::start(move || Ok(Box::new(engine.clone()) as Box<dyn Engine>), KEEP).unwrap()
}

/// Every update until the recording ends, or its channel closes.
pub(super) fn the_rest(updates: &flume::Receiver<Update>) -> Vec<Update> {
    let mut all = Vec::new();
    while let Ok(update) = updates.recv_timeout(TIMEOUT) {
        let ended = matches!(update, Update::Ended(_) | Update::Failed(_));
        all.push(update);
        if ended {
            break;
        }
    }
    all
}

/// Updates until one is `wanted`, which is the last.
pub(super) fn until(updates: &flume::Receiver<Update>, wanted: &Update) -> Vec<Update> {
    let mut seen = Vec::new();
    while !seen.last().is_some_and(|last| last == wanted) {
        match updates.recv_timeout(TIMEOUT) {
            Ok(update) => seen.push(update),
            Err(e) => panic!("{e} waiting for {wanted:?}, after {seen:?}"),
        }
    }
    seen
}

/// Gives `recording` its audio as a microphone does, 10 ms at a time.
pub(super) fn feed(recording: &Recording, audio: &[f32]) {
    for buffer in audio.chunks(160) {
        recording.feed(buffer.to_vec());
    }
}

pub(super) fn hearing(text: &str) -> Update {
    Update::Hearing(text.to_string())
}

pub(super) fn final_text(text: &str) -> Update {
    Update::Final(text.to_string())
}
