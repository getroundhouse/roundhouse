//! Live text as it's said, and each sentence's final text once it has ended.

use super::support::{SCRIPT, dictation, feed, final_text, hearing, the_rest, until};
use crate::dictation::{Ending, Update};
use crate::fake::FakeEngine;
use crate::{AudioIn, Samples};

#[test]
fn live_text_grows_as_words_are_said_and_a_sentence_turns_final_once_the_next_begins() {
    let engine = FakeEngine::new(SCRIPT);
    let audio = engine.saying("fix the flaky test. / run it again");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &audio);

    let first = until(&updates, &final_text("Fix the flaky test."));
    assert!(first.contains(&hearing("fix the")), "{first:?}");
    assert!(first.contains(&hearing("fix the flaky test")), "{first:?}");
    // Only once the next word is heard, which the full stop the live text gives comes with.
    assert!(!first.iter().any(|u| matches!(u, Update::Hearing(text) if text.contains("run"))), "{first:?}");
    // The full stop was the sentence's, which has its own, so the live text starts with the next word. The last word
    // is still held back, as the model holds back what it hasn't heard the end of.
    assert_eq!(until(&updates, &hearing("run it")), [hearing("run"), hearing("run it")]);

    recording.stop();

    assert_eq!(the_rest(&updates), [final_text("Run it again."), hearing(""), Update::Ended(Ending::Heard)]);
}

#[test]
fn a_pause_in_the_middle_of_a_sentence_ends_nothing_and_it_is_heard_whole() {
    let engine = FakeEngine::new(SCRIPT);
    let audio = engine.saying("fix the flaky test / run it again");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &audio);

    let before = until(&updates, &hearing("fix the flaky test run it"));
    assert!(!before.iter().any(|u| matches!(u, Update::Final(_))), "{before:?}");
    recording.stop();
    assert_eq!(
        the_rest(&updates),
        [final_text("Fix the flaky test run it again."), hearing(""), Update::Ended(Ending::Heard)]
    );
}

#[test]
fn a_sentence_whose_final_text_leaves_off_its_full_stop_takes_the_live_texts() {
    let engine = FakeEngine::new(SCRIPT).leaving_off_the_last_full_stop();
    let audio = engine.saying("fix the test. / run it.");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &audio);
    recording.stop();

    // The last too, which the live text ends a sentence with as the recording ends.
    assert_eq!(finals(&the_rest(&updates)), ["Fix the test.", "Run it."]);
}

#[test]
fn a_sentence_heard_whole_as_nothing_goes_in_as_its_live_text() {
    let engine = FakeEngine::new(SCRIPT).hearing_nothing_whole();
    let audio = engine.saying("fix the test. / run it");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &audio);
    recording.stop();

    let rest = the_rest(&updates);
    // As the live text had them: the first with the full stop it gave, the last with none, as it gave none at the end.
    assert_eq!(finals(&rest), ["fix the test.", "run it"]);
    assert_eq!(rest.last(), Some(&Update::Ended(Ending::Heard)));
}

#[test]
fn the_last_sentence_keeps_its_own_ending_where_the_live_text_ends_none() {
    let engine = FakeEngine::new(SCRIPT).leaving_off_the_last_full_stop();
    let audio = engine.saying("fix the test. / run it again");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &audio);
    recording.stop();

    assert_eq!(finals(&the_rest(&updates)), ["Fix the test.", "Run it again"]);
}

#[test]
fn a_long_pause_in_a_sentence_takes_none_of_the_room_the_engine_has() {
    // Heard whole with its pause, the sentence would be too long for the engine.
    let engine = FakeEngine::new(SCRIPT).with_longest_piece(Samples::from_millis(3500));
    let audio = engine.saying("fix the / / / test");
    assert!(audio.len() > Samples::from_millis(4500).0);
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &audio);
    recording.stop();

    assert_eq!(finals(&the_rest(&updates)), ["Fix the test."]);
}

#[test]
fn the_live_text_of_a_sentence_goes_once_it_is_final_however_late_it_comes() {
    // Held back longer than the pause, the first sentence's words come after its piece has ended.
    for held_back in [500, 2500] {
        let engine = FakeEngine::new(SCRIPT).holding_back(Samples::from_millis(held_back));
        let audio = engine.saying("fix the test. / run it");
        let dictation = dictation(engine);
        let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

        feed(&recording, &audio);
        recording.stop();

        let all = the_rest(&updates);
        let at_final = all.iter().position(|u| *u == final_text("Fix the test.")).unwrap();
        let after = &all[at_final..];
        assert!(!after.iter().any(|u| matches!(u, Update::Hearing(text) if text.contains("fix"))), "{all:?}");
        assert!(after.contains(&final_text("Run it.")), "{all:?}");
    }
}

#[test]
fn a_recording_with_no_speech_ends_with_nothing_heard() {
    let engine = FakeEngine::new(SCRIPT);
    let quiet = engine.saying("/ /");
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &quiet);
    recording.stop();

    assert_eq!(the_rest(&updates), [Update::Ended(Ending::NothingHeard)]);
}

#[test]
fn a_sentence_too_long_for_the_engine_is_cut_between_words_and_goes_on_as_the_live_text_has_it() {
    let engine = FakeEngine::new(SCRIPT).with_longest_piece(Samples::from_millis(3000));
    let said = "fix the flaky test run it again please fix the flaky test";
    let audio = engine.saying(said);
    let dictation = dictation(engine);
    let (recording, updates) = dictation.record(AudioIn::Fed, crate::Hints::default());

    feed(&recording, &audio);
    recording.stop();

    let finals = finals(&the_rest(&updates));
    assert!(finals.len() >= 2, "{finals:?}");
    let words: Vec<String> =
        finals.iter().flat_map(|text| text.trim_end_matches('.').split(' ')).map(str::to_lowercase).collect();
    assert_eq!(words.join(" "), said, "no word lost or split between pieces");
    // One sentence, as the live text has it, with no full stop where it was cut, and no capital after one.
    let (last, before) = finals.split_last().unwrap();
    assert!(last.ends_with('.') && before.iter().all(|text| !text.ends_with('.')), "{finals:?}");
    assert!(finals[1..].iter().all(|text| text.starts_with(char::is_lowercase)), "{finals:?}");
}

/// The final text in `updates`.
fn finals(updates: &[Update]) -> Vec<String> {
    updates.iter().filter_map(|u| if let Update::Final(text) = u { Some(text.clone()) } else { None }).collect()
}
