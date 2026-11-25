//! What the app's log says of each recording: that it started, how it ended, and why it failed, but never what was
//! typed, hinted or said.

use dictation::fake::FakeEngine;
use gpui_kit::TestAppContext;
use logging::testing::Logs;
use serde_json::Value;

use super::support::{ended, hearing, ready, ready_with, say};
use crate::support::{Stack, Ui};

/// What's logged of recordings, by its name, with its fields.
fn logged(logs: &Logs) -> Vec<(String, Value)> {
    logs.events()
        .iter()
        .filter_map(|event| Some((event["event"].as_str()?.to_owned(), event["fields"].clone())))
        .filter(|(event, _)| event.starts_with("dictation."))
        .collect()
}

#[gpui_kit::test]
fn a_recording_is_logged_as_it_starts_and_ends_without_what_was_typed_hinted_or_said(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready_with(&mut ui, FakeEngine::new(&["meet", "the", "quokka"]));
    ui.input("Ask WombatService to ");
    let logs = Logs::capture();

    ui.click("dictate");
    say(&mut ui, &engine, "meet the quokka");
    hearing(&mut ui, "meet");
    ui.click(dictation_ui::STOP);
    ui.wait("the final text", |ui| ui.composer_text().ends_with("the quokka."));
    ended(&mut ui);

    let hints = engine.last_hints();
    assert!(hints.words().iter().any(|word| word == "WombatService"), "{:?}", hints.words());
    let events = logged(&logs);
    let [(started, at_start), (finished, at_end)] = events.as_slice() else { panic!("{events:#?}") };
    assert_eq!((started.as_str(), finished.as_str()), ("dictation.started", "dictation.ended"));
    assert_eq!(at_start["place"], "composer");
    assert_eq!(at_start["hinted"], hints.words().len());
    assert_eq!(at_start["recording_id"], at_end["recording_id"]);
    assert_eq!(at_end["how"], "heard");
    assert!(at_end["recorded_ms"].is_u64() && at_end["final_text_ms"].is_u64(), "{at_end}");
    let text = logs.text().to_lowercase();
    for private in ["wombat", "quokka"] {
        assert!(!text.contains(private), "{text}");
    }
}

#[gpui_kit::test]
fn however_a_recording_ends_is_logged_once_and_a_failure_with_why(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    let logs = Logs::capture();

    ui.click("dictate");
    ui.click(dictation_ui::STOP);
    ended(&mut ui);

    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.press("escape");
    ended(&mut ui);

    ui.click("dictate");
    ui.press("secondary-,");
    ui.press("escape");
    ended(&mut ui);

    let failing = ready_with(&mut ui, FakeEngine::new(super::support::SCRIPT).failing());
    ui.click("dictate");
    say(&mut ui, &failing, "fix the test");
    ui.click(dictation_ui::STOP);
    ended(&mut ui);

    let events = logged(&logs);
    let said: Vec<&str> =
        events.iter().map(|(event, fields)| fields["how"].as_str().unwrap_or(event.as_str())).collect();
    let started = "dictation.started";
    assert_eq!(
        said,
        [started, "nothing_heard", started, "cancelled", started, "field_gone", started, "dictation.failed", "failed"]
    );
    let (_, failure) = events.iter().find(|(event, _)| event == "dictation.failed").unwrap();
    assert_eq!(failure["error"], "the model's files are damaged");
}

#[gpui_kit::test]
fn a_model_that_cant_be_prepared_is_logged_with_why(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let logs = Logs::capture();

    let (dir, files) =
        (tempfile::tempdir().unwrap(), models::ModelFiles { repository: "none", revision: "none", files: &[] });
    let broken = |_: &std::path::Path| Err(dictation::EngineError("its files are damaged".to_string()));
    super::support::use_model(&mut ui, dir.path(), files, "", broken);
    ui.cx.update(|cx| desktop::dictating::set_on(true, cx));
    ui.wait("it to be logged", |_| !logged(&logs).is_empty());

    let events = logged(&logs);
    let [(event, fields)] = events.as_slice() else { panic!("{events:#?}") };
    assert_eq!(event, "dictation.unready");
    assert_eq!((&fields["step"], &fields["error"]), (&"prepare".into(), &"its files are damaged.".into()));
}
