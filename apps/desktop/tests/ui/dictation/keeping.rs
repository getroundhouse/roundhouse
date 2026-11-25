//! What's dictated is kept only once the person chooses, however it ends, and deleted when they ask.

use std::path::PathBuf;

use desktop::dictating::{DELETE_RECORDINGS, RECORDINGS_STATUS};
use dictation::fake::FakeEngine;
use gpui_kit::{ScrollDelta, SharedString, TestAppContext, point, px};
use kept_recordings::{AUDIO, FILE, Field, HEADER_BYTES, How, Kept, Text};

use super::holding_space::{hold_on_to_dictate, hold_space_to_listen};
use super::support::{SCRIPT, ended, hearing, hold_option_space, let_go, open_voice_settings, ready, ready_with, say};
use crate::support::{Stack, Ui};

const KEEP: &str = "Keep recordings";

fn folder(stack: &Stack) -> PathBuf {
    stack.hub.paths.data_dir.join("recordings")
}

/// Every kept recording that has ended, in the order they started, with how many samples of audio each has.
fn kept(stack: &Stack) -> Vec<(Kept, usize)> {
    let Ok(entries) = std::fs::read_dir(folder(stack)) else { return Vec::new() };
    let mut kept: Vec<(Kept, usize)> = entries
        .map(|entry| entry.unwrap().path())
        .filter_map(|dir| {
            let file: Kept = serde_json::from_slice(&std::fs::read(dir.join(FILE)).ok()?).unwrap();
            let audio = std::fs::metadata(dir.join(AUDIO)).ok()?.len();
            file.ended.is_some().then(|| (file, (usize::try_from(audio).unwrap() - HEADER_BYTES) / 4))
        })
        .collect();
    kept.sort_by_key(|(kept, _)| kept.started_at_ms);
    kept
}

/// Turns Keep recordings on, or off, in Settings.
fn choose(ui: &mut Ui) {
    open_voice_settings(ui);
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    ui.click_label(KEEP);
    ui.press("escape");
}

#[gpui_kit::test]
fn a_recording_is_kept_whole_with_what_it_was_heard_with_and_what_was_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready_with(&mut ui, FakeEngine::new(SCRIPT));
    choose(&mut ui);
    ui.input("Ask WombatService to ");

    ui.click("dictate");
    let audio = engine.saying("fix the test");
    super::support::hear(&mut ui, &audio);
    hearing(&mut ui, "fix");
    ui.click(dictation_ui::STOP);
    ended(&mut ui);
    ui.wait("it to be kept", |_| !kept(&stack).is_empty());

    let kept = kept(&stack);
    let [(kept, samples)] = kept.as_slice() else { panic!("{kept:#?}") };
    assert_eq!(*samples, audio.len());
    assert_eq!(kept.field, Field::Composer);
    let again = dictation::Hints::from_kept(
        kept.hints.iter().map(|hint| dictation::Hint { text: hint.text.clone(), spelled: hint.spelled }),
    );
    assert_eq!(again, engine.last_hints(), "it can be heard again with the same hints");
    assert_eq!(kept.chat, Some(ui.active_chat().to_string()));
    assert!(kept.workspace.is_some() && !kept.said_out_loud);
    let finals: Vec<&Text> = kept.said.iter().map(|said| &said.text).filter(|t| matches!(t, Text::Final(_))).collect();
    assert_eq!(finals, [&Text::Final("Fix the test.".to_string())]);
    assert!(kept.said.iter().any(|said| matches!(&said.text, Text::Live(live) if live.starts_with("fix"))));
    assert_eq!(kept.ended.as_ref().unwrap().how, How::Heard);
    assert!(kept.stopped_ms.is_some());
}

#[gpui_kit::test]
fn one_thats_cancelled_fails_or_loses_its_field_is_kept_with_how_it_ended(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    choose(&mut ui);

    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.press("escape");
    ended(&mut ui);

    ui.click("dictate");
    ui.press("secondary-,");
    ui.press("escape");
    ended(&mut ui);

    let failing = ready_with(&mut ui, FakeEngine::new(SCRIPT).failing());
    ui.click("dictate");
    say(&mut ui, &failing, "fix the test");
    ui.click(dictation_ui::STOP);
    ended(&mut ui);

    ui.wait("all three to be kept", |_| kept(&stack).len() == 3);
    let kept = kept(&stack);
    let how: Vec<How> = kept.iter().map(|(kept, _)| kept.ended.as_ref().unwrap().how).collect();
    assert_eq!(how, [How::Cancelled, How::FieldGone, How::Failed]);
    assert_eq!(kept[2].0.ended.as_ref().unwrap().error.as_deref(), Some("the model's files are damaged"));
    assert_eq!(kept[0].1, engine.saying("fix the test").len(), "a cancelled one's audio is whole");
}

#[gpui_kit::test]
fn nothing_is_written_while_its_off_or_for_a_press_let_go_of_before_it_dictates(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);

    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    let_go(&mut ui);
    ended(&mut ui);
    assert!(!folder(&stack).exists());

    choose(&mut ui);
    hold_space_to_listen(&mut ui);
    say(&mut ui, &engine, "fix the test");
    let_go(&mut ui);
    assert!(!folder(&stack).exists());

    // Turned off since it began to listen.
    hold_space_to_listen(&mut ui);
    ui.cx.update(|cx| desktop::dictating::keep_recordings(false, cx));
    hold_on_to_dictate(&mut ui);
    assert!(super::support::dictating(&mut ui));
    let_go(&mut ui);
    ended(&mut ui);
    assert!(!folder(&stack).exists());
}

#[gpui_kit::test]
fn turning_it_off_drops_the_one_underway_and_keeps_the_rest_until_theyre_deleted(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    choose(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.click(dictation_ui::STOP);
    ended(&mut ui);
    ui.wait("it to be kept", |_| kept(&stack).len() == 1);

    ui.click("dictate");
    say(&mut ui, &engine, "in hub");
    ui.cx.update(|cx| desktop::dictating::keep_recordings(false, cx));
    ui.wait("the one underway to be dropped", |_| std::fs::read_dir(folder(&stack)).unwrap().count() == 1);
    ui.press("escape");
    ended(&mut ui);
    assert_eq!(kept(&stack).len(), 1);

    open_voice_settings(&mut ui);
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    ui.wait("how much room they take", |ui| ui.has(RECORDINGS_STATUS));
    ui.click(DELETE_RECORDINGS);
    ui.click(SharedString::from(format!("{DELETE_RECORDINGS}-keep")));
    assert!(folder(&stack).exists());
    ui.click(DELETE_RECORDINGS);
    ui.click(SharedString::from(format!("{DELETE_RECORDINGS}-act")));
    ui.wait("them to be gone", |ui| !ui.has(RECORDINGS_STATUS));
    assert!(!folder(&stack).exists());
}
