//! Hearing recordings again with `FakeEngine`, from files written as the app writes them.

use std::path::Path;

use dictation::fake::FakeEngine;
use dictation::{AudioIn, Dictation, Engine, Hint, Hints, Language, SAMPLE_RATE, Update};
use hint_words::HintWord;
use kept_recordings::{AUDIO, Ended, FILE, Field, How, Kept, SENT, SENT_VERSION, Said, Sent, SentFrom, Text, VERSION};

use super::comparing::compare;
use super::hearing::{Run, Settings, Stamp, hear_all};
use super::recordings;
use super::sending::sent;
use crate::hearing::Hearer;

const SCRIPT: &[&str] = &["fix", "the", "test", "run", "it"];
const MODEL: &str = "the model";

fn kept(id: &str, started_at_ms: u64, finals: &[String]) -> Kept {
    let said = |text: &String| Said { at_ms: 0, text: Text::Final(text.clone()) };
    Kept {
        version: VERSION,
        id: id.to_string(),
        started_at_ms,
        build: "dev".to_string(),
        model: MODEL.to_string(),
        language: "auto".to_string(),
        hint_boost: 2.0,
        hints: vec![HintWord { text: "sqlite-vec".to_string(), spelled: true }],
        picked_from: None,
        field: Field::Composer,
        workspace: None,
        chat: None,
        said_out_loud: false,
        device: None,
        said: finals.iter().map(said).collect(),
        stopped_ms: None,
        ended: Some(Ended { at_ms: 0, how: How::Heard, error: None }),
    }
}

/// Writes `kept` and its audio into `folder` as the app does.
fn write(folder: &Path, kept: &Kept, audio: &[f32]) {
    let dir = folder.join(&kept.id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(FILE), serde_json::to_vec_pretty(kept).unwrap()).unwrap();
    let data: Vec<u8> = audio.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let header = kept_recordings::header(SAMPLE_RATE, u32::try_from(data.len()).unwrap());
    std::fs::write(dir.join(AUDIO), [&header[..], &data].concat()).unwrap();
}

/// Dictates `said` as the app does, and gives the audio it copies and the final texts.
fn dictate(engine: &FakeEngine, said: &str) -> (Vec<f32>, Vec<String>) {
    let load = engine.clone();
    let hour = std::time::Duration::from_hours(1);
    let dictation = Dictation::start(move || Ok(Box::new(load.clone()) as Box<dyn Engine>), hour).unwrap();
    let hints = Hints::from_kept([Hint { text: "sqlite-vec".to_string(), spelled: true }]);
    let copied = dictation.record_copying(AudioIn::Fed, hints, Language::default());
    copied.recording.feed(engine.saying(said));
    copied.recording.stop();
    let mut finals = Vec::new();
    for update in &copied.updates {
        match update {
            Update::Final(text) => finals.push(text),
            Update::Ended(_) => break,
            Update::Failed(error) => panic!("{error}"),
            Update::Opened(_) | Update::Hearing(_) => {}
        }
    }
    (copied.audio.iter().flatten().collect(), finals)
}

fn stamp(settings: Settings) -> Stamp {
    let text = |text: &str| text.to_string();
    Stamp { settings, model: text(MODEL), code: text("a commit"), machine: text("a test") }
}

/// Hears what's in `folder` into the run in `out`, and gives the boost each hearer was started with.
fn hear(folder: &Path, out: &Path, settings: Settings, engine: &FakeEngine) -> Vec<f32> {
    let mut run = Run::open(out, stamp(settings)).unwrap();
    let mut boosts = Vec::new();
    hear_all(&recordings::read(folder).unwrap(), &mut run, |boost| {
        boosts.push(boost);
        let engine = engine.clone();
        Hearer::start(move || Ok(Box::new(engine.clone()) as Box<dyn Engine>))
    })
    .unwrap();
    boosts
}

#[test]
fn a_recording_is_heard_again_as_the_same_final_text_with_the_hints_it_had() {
    let (data, engine) = (tempfile::tempdir().unwrap(), FakeEngine::new(SCRIPT));
    let (folder, out) = (data.path().join("recordings"), data.path().join("run"));
    let (audio, finals) = dictate(&engine, "fix the test. / run it.");
    assert_eq!(finals, ["Fix the test.", "Run it."]);
    write(&folder, &kept("r1", 1, &finals), &audio);
    std::fs::write(folder.join("unsent.json"), "[]").unwrap();

    let again = FakeEngine::new(SCRIPT);
    assert_eq!(hear(&folder, &out, Settings::default(), &again), [2.0], "with the boost it was heard with");

    assert_eq!(again.last_hints(), engine.last_hints());
    let heard = Run::read(&out).unwrap().heard;
    assert_eq!(heard[0].final_text, "Fix the test. Run it.");
    let compared = compare(&out, None).unwrap();
    assert!(compared.starts_with("1 came out the same, 0 with other capitals or marks, and 0 with"), "{compared}");
}

#[test]
fn settings_given_take_the_place_of_those_it_had_and_those_alike_are_heard_together() {
    let (data, engine) = (tempfile::tempdir().unwrap(), FakeEngine::new(SCRIPT));
    let folder = data.path().join("recordings");
    let audio = engine.saying("fix the test.");
    for (id, at, boost) in [("r1", 1, 2.0), ("r2", 2, 1.0), ("r3", 3, 2.0)] {
        write(&folder, &Kept { hint_boost: boost, ..kept(id, at, &[]) }, &audio);
    }

    assert_eq!(hear(&folder, &data.path().join("as-kept"), Settings::default(), &engine), [1.0, 2.0]);
    assert!(engine.last_hints().words().contains(&"sqlite-vec".to_string()));
    let other = Settings { boost: Some(3.0), no_hints: true };
    assert_eq!(hear(&folder, &data.path().join("other"), other, &engine), [3.0]);
    assert!(engine.last_hints().words().is_empty());
}

#[test]
fn a_run_goes_on_from_where_it_stopped_and_refuses_other_settings() {
    let (data, engine) = (tempfile::tempdir().unwrap(), FakeEngine::new(SCRIPT));
    let (folder, out) = (data.path().join("recordings"), data.path().join("run"));
    write(&folder, &kept("r1", 1, &[]), &engine.saying("fix the test."));
    hear(&folder, &out, Settings::default(), &engine);
    write(&folder, &kept("r2", 2, &[]), &engine.saying("run it."));
    let heard = out.join("heard.jsonl");
    let whole = std::fs::read_to_string(&heard).unwrap();
    std::fs::write(&heard, format!("{whole}{{\"id\": \"r2")).unwrap();

    hear(&folder, &out, Settings::default(), &engine);
    assert_eq!(hear(&folder, &out, Settings::default(), &engine), Vec::<f32>::new(), "nothing's left to hear");

    let ids: Vec<String> = Run::read(&out).unwrap().heard.into_iter().map(|heard| heard.id).collect();
    assert_eq!(ids, ["r1", "r2"]);
    assert!(Run::open(&out, stamp(Settings { boost: Some(3.0), no_hints: false })).is_err());
    assert!(Run::open(&out, Stamp { code: "another".to_string(), ..stamp(Settings::default()) }).is_err());
}

#[test]
fn only_what_came_out_differently_is_listed_in_the_order_it_was_dictated() {
    let (data, engine) = (tempfile::tempdir().unwrap(), FakeEngine::new(SCRIPT));
    let (folder, out) = (data.path().join("recordings"), data.path().join("run"));
    let written = |text: &str| [text.to_string()];
    let said = engine.saying("fix the test.");
    write(&folder, &kept("later", 9, &written("Fix the tests.")), &said);
    write(&folder, &kept("same", 2, &written("Fix the test.")), &said);
    write(&folder, &kept("marks", 3, &written("Fix the test")), &said);
    // Heard after the rest, by its boost.
    write(&folder, &Kept { hint_boost: 5.0, ..kept("earlier", 1, &written("Fix a test.")) }, &said);
    let ended = Some(Ended { at_ms: 0, how: How::Cancelled, error: None });
    write(&folder, &Kept { ended, ..kept("cancelled", 4, &[]) }, &said);
    write(&folder, &Kept { ended: None, ..kept("app-went", 5, &[]) }, &said);
    write(&folder, &Kept { model: "another".to_string(), ..kept("older", 6, &written("Fix.")) }, &said);
    hear(&folder, &out, Settings::default(), &engine);

    let compared = compare(&out, None).unwrap();
    let first = "1 came out the same, 1 with other capitals or marks, and 2 with other words.";
    assert!(compared.starts_with(first), "{compared}");
    let at = |id: &str| compared.find(&format!("\n{id}, ")).unwrap_or_else(|| panic!("{id} in {compared}"));
    assert!(at("earlier") < at("marks") && at("marks") < at("later"), "{compared}");
    assert!(compared.contains("  was: Fix a test.\n  now: Fix the test.\n  fix [a→the] test\n"), "{compared}");
    assert!(!compared.contains("\nsame, "), "{compared}");
    assert!(compared.contains("\n2 aren't set against the app's, which didn't hear them to their end."), "{compared}");
    assert!(compared.contains("\n1 aren't set against the app's, which heard them with another model."), "{compared}");
}

#[test]
fn a_run_is_set_against_another() {
    let (data, engine) = (tempfile::tempdir().unwrap(), FakeEngine::new(SCRIPT));
    let (folder, then, now) = (data.path().join("recordings"), data.path().join("then"), data.path().join("now"));
    write(&folder, &kept("r1", 1, &[]), &engine.saying("fix the test."));
    hear(&folder, &then, Settings::default(), &engine);
    write(&folder, &kept("r2", 2, &[]), &engine.saying("run it."));
    let other = FakeEngine::new(&["mix", "the", "test", "run", "it"]);
    hear(&folder, &now, Settings { boost: Some(3.0), no_hints: false }, &other);

    let compared = compare(&now, Some(&then)).unwrap();
    assert!(compared.contains("and 1 with other words."), "{compared}");
    assert!(compared.contains("[fix→mix] the test"), "{compared}");
    assert!(compared.contains("\n1 weren't heard in the other run."), "{compared}");
}

#[test]
fn one_the_app_went_before_the_end_of_is_heard_as_far_as_it_got() {
    let (data, engine) = (tempfile::tempdir().unwrap(), FakeEngine::new(SCRIPT));
    let (folder, out) = (data.path().join("recordings"), data.path().join("run"));
    write(&folder, &Kept { ended: None, ..kept("r1", 1, &[]) }, &engine.saying("fix the test."));
    let path = folder.join("r1").join(AUDIO);
    let mut audio = std::fs::read(&path).unwrap();
    audio[..kept_recordings::HEADER_BYTES]
        .copy_from_slice(&kept_recordings::header(SAMPLE_RATE, kept_recordings::MAX_DATA_BYTES));
    audio.extend([0, 0]);
    std::fs::write(&path, audio).unwrap();

    hear(&folder, &out, Settings::default(), &engine);

    assert_eq!(Run::read(&out).unwrap().heard[0].final_text, "Fix the test.");
}

/// Writes what was sent into `folder` as the app does, each with when, and the chat it was sent from.
fn write_sent(folder: &Path, all: &[(u64, &str, &str)]) {
    let line = |&(sent_at_ms, chat, text): &(u64, &str, &str)| {
        let chat = Some(chat.to_string());
        let from = SentFrom { field: Field::Composer, workspace: None, chat, said_out_loud: false };
        let sent = Sent { version: SENT_VERSION, sent_at_ms, from, text: text.to_string() };
        serde_json::to_string(&sent).unwrap() + "\n"
    };
    std::fs::write(folder.join(SENT), all.iter().map(line).collect::<String>()).unwrap();
}

#[test]
fn what_became_of_each_recording_is_in_the_next_thing_sent_from_where_it_was_dictated() {
    let data = tempfile::tempdir().unwrap();
    let folder = data.path();
    let dictated = |id: &str, at: u64, chat: &str, text: &str| {
        let finals = if text.is_empty() { Vec::new() } else { vec![text.to_string()] };
        write(folder, &Kept { chat: Some(chat.to_string()), ..kept(id, at, &finals) }, &[]);
    };
    dictated("typed-around", 10, "s1", "Fix the test.");
    dictated("second", 11, "s1", "Then run it all.");
    dictated("other-chat", 12, "s2", "Fix the test.");
    dictated("changed", 30, "s1", "Look at the work trees.");
    dictated("deleted", 31, "s1", "Never mind about that one.");
    dictated("marks", 32, "s1", "and run cargo test");
    dictated("nothing", 33, "s1", "");
    dictated("later", 60, "s1", "Fix the test.");
    write_sent(
        folder,
        &[
            (50, "s1", "Look at the worktrees. And run cargo test."),
            (20, "s1", "Please Fix the test. Then run it all. Thanks"),
            (70, "s2", "Fix the tests."),
        ],
    );

    let listed = sent(folder).unwrap();

    let counts = "2 were sent as they were written, 1 with other capitals or marks, 2 changed, and 1 weren't in what was \
                  sent. Nothing has been sent since 1, and nothing was written of 1.";
    assert!(listed.starts_with(counts), "{listed}");
    let changed = "\nchanged\n  wrote: Look at the work trees.\n  sent: Look at the worktrees\n  look at the";
    assert!(listed.contains(changed), "{listed}");
    assert!(listed.contains("\nmarks\n  wrote: and run cargo test\n  sent: And run cargo test\n"), "{listed}");
    assert!(listed.contains("\ndeleted\n  wrote: Never mind about that one.\n  not in what was sent\n"), "{listed}");
    assert!(listed.contains("\nother-chat\n  wrote: Fix the test.\n  sent: Fix the tests\n"), "{listed}");
    assert!(!listed.contains("second") && !listed.contains("typed-around"), "{listed}");
}

#[test]
fn a_line_of_what_was_sent_cut_short_as_the_app_went_is_left_out() {
    let data = tempfile::tempdir().unwrap();
    let folder = data.path();
    write(folder, &Kept { chat: Some("s1".to_string()), ..kept("r1", 1, &["Fix it.".to_string()]) }, &[]);
    write_sent(folder, &[(2, "s1", "Fix it.")]);
    let path = folder.join(SENT);
    std::fs::write(&path, std::fs::read_to_string(&path).unwrap() + "{\"version\": 1").unwrap();

    assert!(sent(folder).unwrap().starts_with("1 were sent as they were written,"));

    std::fs::remove_file(&path).unwrap();
    assert!(sent(folder).unwrap().contains("Nothing has been sent since 1,"));
}
