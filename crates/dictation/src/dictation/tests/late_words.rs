//! Words an engine gives late: after their piece was cut off as silence, after pieces that followed it, or only once
//! the recording ends, said past its end. A model gives its words late when it holds back what it hasn't heard the
//! end of, and hears words a quiet speaker says where the level says nothing was. Its marks, such as `.`, count as
//! nothing said.

use super::support::{KEEP, feed, final_text, hearing, the_rest, until};
use crate::dictation::{Dictation, Ending, Update};
use crate::fake::FakeEngine;
use crate::{AudioIn, Engine, EngineError, Heard, Listening, Samples};

/// A word an engine gives: what it is, where in the recording it was said, and once how much of it has been heard
/// it's given, or `None` for when the recording ends.
#[derive(Clone, Copy)]
struct Word {
    text: &'static str,
    said: Samples,
    given: Option<Samples>,
}

fn word(text: &'static str, said_ms: usize, given_ms: Option<usize>) -> Word {
    Word { text, said: Samples::from_millis(said_ms), given: given_ms.map(Samples::from_millis) }
}

/// An engine that hears its words whatever the audio, and whose final text says whether a piece was loud.
#[derive(Clone)]
struct Telling(Vec<Word>);

struct TellingListening {
    /// The words still to give.
    words: Vec<Word>,
    heard: usize,
}

impl Engine for Telling {
    fn longest_piece(&self) -> Samples {
        Samples::from_secs(15)
    }

    fn listen(&self, _: &crate::Hints) -> Box<dyn Listening + '_> {
        Box::new(TellingListening { words: self.0.clone(), heard: 0 })
    }

    fn transcribe(&self, piece: &[f32], _: &crate::Hints) -> Result<String, EngineError> {
        let loud = piece.iter().any(|x| x.abs() > 0.1);
        Ok(if loud { "Loudly said." } else { "Quietly said." }.to_string())
    }
}

impl TellingListening {
    fn give(&mut self, now: impl Fn(&Word) -> bool) -> Vec<Heard> {
        let (given, rest) = self.words.iter().partition::<Vec<Word>, _>(|word| now(word));
        self.words = rest;
        given.into_iter().map(|word| Heard { text: word.text.to_string(), at: word.said }).collect()
    }
}

impl Listening for TellingListening {
    fn hear(&mut self, audio: &[f32]) -> Result<Vec<Heard>, EngineError> {
        self.heard += audio.len();
        let heard = self.heard;
        Ok(self.give(|word| word.given.is_some_and(|given| given.0 <= heard)))
    }

    fn finish(&mut self) -> Result<Vec<Heard>, EngineError> {
        Ok(self.give(|_| true))
    }

    fn heard_through(&self) -> Samples {
        Samples(self.words.iter().map(|word| word.said.0).fold(self.heard, usize::min))
    }
}

fn telling(words: &[Word]) -> Dictation {
    let words = words.to_vec();
    Dictation::start(move || Ok(Box::new(Telling(words.clone())) as Box<dyn Engine>), KEEP).unwrap()
}

fn silence(millis: usize) -> Vec<f32> {
    vec![0.0; Samples::from_millis(millis).0]
}

#[test]
fn words_heard_where_the_level_says_nothing_was_said_still_turn_final() {
    let dictation = telling(&[word(" quietly", 500, Some(600))]);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &silence(1000));
    recording.stop();

    assert_eq!(
        the_rest(&updates),
        [hearing("quietly"), final_text("Quietly said."), hearing(""), Update::Ended(Ending::Heard)]
    );
}

#[test]
fn a_quiet_word_given_after_its_piece_was_cut_off_as_silence_still_turns_final() {
    // Two seconds with no speech are cut off at 1.71 s, before the word is given.
    let dictation = telling(&[word(" quietly", 1600, Some(2100))]);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &silence(3000));
    recording.stop();

    assert_eq!(
        the_rest(&updates),
        [hearing("quietly"), final_text("Quietly said."), hearing(""), Update::Ended(Ending::Heard)]
    );
}

#[test]
fn sentences_turn_final_in_order_and_their_live_text_shows_until_they_do() {
    // A quiet word in silence cut off at 1.71 s, and the full stop after it, then a loud one at 2.1 s, which ends its
    // piece at 2.7 s, before the quiet word is given, and the loud one with it, since words come in order.
    let words = [word(" quietly", 1600, Some(3500)), word(".", 1650, Some(3500)), word(" loudly", 2200, Some(3500))];
    let dictation = telling(&words);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());
    let mut audio = silence(2100);
    audio.extend(FakeEngine::new(&["loud"]).saying("loud /"));

    feed(&recording, &audio);

    assert_eq!(until(&updates, &final_text("Quietly said.")), [final_text("Quietly said.")]);
    recording.stop();
    assert_eq!(
        the_rest(&updates),
        [hearing("loudly"), final_text("Loudly said."), hearing(""), Update::Ended(Ending::Heard)]
    );
}

#[test]
fn a_full_stop_given_alone_after_a_sentence_is_nothing_said() {
    // The sentence's full stop, given in silence cut off after it, with no word after it.
    let words = [word(" loudly", 100, Some(500)), word(".", 3100, Some(3200))];
    let dictation = telling(&words);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());
    let mut audio = FakeEngine::new(&["loud"]).saying("loud");
    audio.extend(silence(4000));

    feed(&recording, &audio);
    recording.stop();

    // Heard whole on its own, the silence would say "Quietly said.".
    assert_eq!(
        the_rest(&updates),
        [hearing("loudly"), hearing("loudly."), final_text("Loudly said."), hearing(""), Update::Ended(Ending::Heard)]
    );
}

#[test]
fn a_word_given_as_said_past_the_end_is_the_last_pieces() {
    let dictation = telling(&[word(" late", 1100, None)]);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &silence(1000));
    recording.stop();

    assert_eq!(the_rest(&updates), [final_text("Quietly said."), Update::Ended(Ending::Heard)]);
}
