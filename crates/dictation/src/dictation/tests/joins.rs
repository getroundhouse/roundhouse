//! How sentences join, through the whole loop: where the live text ends one, and how one too long to hear whole is cut
//! and carries on. `joins.rs` tests the rules alone.

use std::sync::Arc;

use parking_lot::Mutex;

use super::support::{KEEP, SCRIPT, dictation, feed, the_rest};
use crate::dictation::{Dictation, Update};
use crate::fake::FakeEngine;
use crate::{AudioIn, Engine, EngineError, Heard, Listening, Samples};

/// How long after a word is said a scripted engine gives it.
const HELD_BACK: Samples = Samples::from_millis(500);

/// Live text a scripted engine gives, and where in the recording it was said.
#[derive(Clone, Copy)]
struct Said {
    text: &'static str,
    at: Samples,
}

fn said(text: &'static str, at_ms: usize) -> Said {
    Said { text, at: Samples::from_millis(at_ms) }
}

/// An engine whose live text, given in order, and each final text in turn, the test scripts, whatever the audio.
#[derive(Clone)]
struct Scripted {
    live: Vec<Said>,
    finals: Arc<Mutex<Vec<&'static str>>>,
    longest: Samples,
}

struct ScriptedListening {
    live: Vec<Said>,
    heard: Samples,
}

impl Engine for Scripted {
    fn longest_piece(&self) -> Samples {
        self.longest
    }

    fn listen(&self, _: &crate::Hints, _: crate::Language) -> Box<dyn Listening + '_> {
        Box::new(ScriptedListening { live: self.live.clone(), heard: Samples(0) })
    }

    fn transcribe(&self, _: &[f32], _: &crate::Hints, _: crate::Language) -> Result<String, EngineError> {
        let mut finals = self.finals.lock();
        Ok(if finals.is_empty() { String::new() } else { finals.remove(0).to_string() })
    }
}

impl ScriptedListening {
    fn give(&mut self, ready: usize) -> Vec<Heard> {
        self.live.drain(..ready).map(|said| Heard { text: said.text.to_string(), at: said.at }).collect()
    }
}

impl Listening for ScriptedListening {
    fn hear(&mut self, audio: &[f32]) -> Result<Vec<Heard>, EngineError> {
        self.heard = self.heard + Samples(audio.len());
        let ready = self.live.iter().take_while(|said| said.at + HELD_BACK <= self.heard).count();
        Ok(self.give(ready))
    }

    fn finish(&mut self) -> Result<Vec<Heard>, EngineError> {
        Ok(self.give(self.live.len()))
    }

    fn heard_through(&self) -> Samples {
        Samples(self.heard.0.saturating_sub(HELD_BACK.0))
    }
}

fn scripted(live: &[Said], finals: &[&'static str], longest: Samples) -> Dictation {
    let engine = Scripted { live: live.to_vec(), finals: Arc::new(Mutex::new(finals.to_vec())), longest };
    Dictation::start(move || Ok(Box::new(engine.clone()) as Box<dyn Engine>), KEEP).unwrap()
}

/// What's heard of `audio`, a recording, by an engine scripted with `live` and `finals`: the final text.
fn finals_of(audio: &[f32], live: &[Said], finals: &[&'static str], longest: Samples) -> Vec<String> {
    let dictation = scripted(live, finals, longest);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default(), crate::Language::Auto);
    feed(&recording, audio);
    recording.stop();
    finals_in(&the_rest(&updates))
}

fn finals_in(updates: &[Update]) -> Vec<String> {
    updates.iter().filter_map(|u| if let Update::Final(text) = u { Some(text.clone()) } else { None }).collect()
}

/// Two pieces, 1.14 s of sound, a pause, and 1.14 s more, so 1.44 s and 2.16 s once cut, and 3.6 s together.
fn two_pieces() -> Vec<f32> {
    FakeEngine::new(&["loud"]).saying("loud loud loud / loud loud loud")
}

#[test]
fn a_question_or_exclamation_mark_the_live_text_gives_ends_a_sentence() {
    for mark in ["?", "!"] {
        let live = [said(" should", 100), said(" we", 500), said(mark, 2500), said(" run", 2500), said(" it", 2900)];
        let finals = finals_of(&two_pieces(), &live, &["Should we", "Run it."], Samples::from_secs(15));
        assert_eq!(finals, [format!("Should we{mark}"), "Run it.".to_string()]);
    }
}

#[test]
fn a_sentence_cut_short_takes_the_live_texts_comma_and_keeps_the_capital_after_it() {
    let live = [said(" we", 100), said(" should", 500), said(",", 1000), said(" I", 2500), said(" think", 2900)];
    let finals = finals_of(&two_pieces(), &live, &["We should.", "I think."], Samples::from_millis(2500));
    assert_eq!(finals, ["We should,", "I think."]);
}

#[test]
fn a_symbol_at_the_end_of_a_sentence_cut_short_is_the_words_and_not_repeated() {
    let live = [said(" fifty", 100), said("%", 900), said(" and", 2500), said(" more", 2900)];
    let finals = finals_of(&two_pieces(), &live, &["Fifty%.", "And more."], Samples::from_millis(2500));
    assert_eq!(finals, ["Fifty%", "and more."]);
}

#[test]
fn a_sentence_counts_all_its_pieces_against_what_the_engine_takes() {
    // Pieces of about 1.0, 1.9, 1.9 and 1.7 s: the first two fit in 4 s together, and the third doesn't with them.
    let engine = FakeEngine::new(SCRIPT).with_longest_piece(Samples::from_millis(4000));
    let audio = engine.saying("fix the / flaky test / run it / again please");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default(), crate::Language::Auto);

    feed(&recording, &audio);
    recording.stop();

    assert_eq!(finals_in(&the_rest(&updates)), ["Fix the flaky test", "run it again please."]);
}

#[test]
fn a_sentence_after_one_cut_short_begins_with_its_own_capital() {
    let engine = FakeEngine::new(SCRIPT).with_longest_piece(Samples::from_millis(3000));
    let audio = engine.saying("fix the flaky test run it again please fix the flaky test. / run it");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default(), crate::Language::Auto);

    feed(&recording, &audio);
    recording.stop();

    let finals = finals_in(&the_rest(&updates));
    assert!(finals.len() >= 3, "{finals:?}");
    assert_eq!(finals.last().map(String::as_str), Some("Run it."), "{finals:?}");
    assert!(finals[1].starts_with(char::is_lowercase), "{finals:?}");
}
