//! The native treatment keeps recording actions alone, follows real audio and stops requesting frames at rest.

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Focusable as _, KeyDownEvent, KeyUpEvent, Keystroke, PlatformInput, TestAppContext};

use super::support::{ended, hearing, ready, say, sent};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn a_keyboard_started_recording_keeps_escape_and_typing_in_the_input(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Keep this");
    ui.press_at_once(&["tab", "tab", "tab"]);
    assert_eq!(
        ui.cx.update_window(ui.handle, |_, window, _| window.find("dictate").focused()).unwrap(),
        Some(true),
        "Tab reaches the microphone after Attach and Model"
    );
    press_control(&mut ui, "space");
    assert!(ui.has(dictation_ui::STOP));
    let pane = ui.pane().unwrap();
    assert!(
        ui.cx
            .update_window(ui.handle, |_, window, cx| pane.read(cx).composer.focus_handle(cx).is_focused(window))
            .unwrap(),
        "recording starts with the input focused"
    );
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    press_control(&mut ui, "escape");

    assert!(!ui.has(dictation_ui::STOP), "Escape reaches the field after its microphone disappears");
    ui.input(" here");
    assert_eq!(ui.composer_text(), "Keep this here");

    ui.open_new_workspace();
    ui.input("Keep the prompt");
    ui.press_at_once(&["tab", "tab", "tab", "tab"]);
    assert_eq!(
        ui.cx
            .update_window(ui.handle, |_, window, _| window.within("new-workspace").find("dictate").focused())
            .unwrap(),
        Some(true),
        "the dialog adds an agent picker before the model"
    );
    press_control(&mut ui, "space");
    assert!(ui.has(dictation_ui::STOP));
    press_control(&mut ui, "escape");
    assert!(ui.has_dialog() && !ui.has(dictation_ui::STOP));
    ui.input(" here");
    assert_eq!(ui.dialog_text(), "Keep the prompt here");
}

/// Native control keys, without forcing a whole-window redraw that could repair focus left on a removed button.
fn press_control(ui: &mut Ui<'_>, key: &'static str) {
    let keystroke = Keystroke::parse(key).unwrap();
    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke }), cx);
        })
        .unwrap();
    ui.frame();
}

#[gpui_kit::test]
fn recording_replaces_the_toolbar_and_restores_it_after_finish_or_cancel(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Keep this");
    ui.click("dictate");

    for id in ["add", "model-picker", "send", "dictate"] {
        assert!(!ui.has(id), "{id} is unrelated to recording");
    }
    assert!(ui.has(dictation_ui::CANCEL) && ui.has(dictation_ui::STOP));
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.click(dictation_ui::STOP);
    ended(&mut ui);

    assert_eq!(ui.composer_text(), "Keep this Fix the test.");
    assert!(!sent(&ui));
    for id in ["add", "model-picker", "send", "dictate"] {
        assert!(ui.has(id), "{id} returns after recording");
    }
    ui.click("dictate");
    say(&mut ui, &engine, "in hub");
    hearing(&mut ui, "in");
    ui.click(dictation_ui::CANCEL);
    ui.input(" here");
    assert_eq!(ui.composer_text(), "Keep this Fix the test. here", "cancel restores text and input focus");
    assert!(ui.has("send") && !ui.has(dictation_ui::STOP));
}

#[gpui_kit::test]
fn the_line_moves_with_audio_and_rests_in_silence_after_cancel_and_under_reduce_motion(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    ui.allow_motion();
    ui.click("dictate");
    ui.settle(Duration::from_millis(20));
    assert_eq!(ui.play(Duration::from_millis(20)), 0, "listening in silence has no loop");
    let pane = ui.pane().unwrap();
    ui.cx.update(|cx| pane.read(cx).hear(&vec![0.1; 480], cx));
    ui.wait("audio to wake the line", |ui| ui.play(Duration::from_millis(20)) > 0);
    assert!(ui.play(Duration::from_millis(200)) > 0, "a sustained sound keeps its ripple moving");

    ui.cx.update(|cx| pane.read(cx).hear(&vec![0.; 480], cx));
    ui.settle(Duration::from_millis(20));
    assert_eq!(ui.play(Duration::from_millis(20)), 0);
    ui.cx.update(|cx| cx.set_reduce_motion(true));
    ui.cx.update(|cx| pane.read(cx).hear(&vec![0.1; 480], cx));
    ui.settle(Duration::from_millis(20));
    assert_eq!(ui.play(Duration::from_millis(200)), 0, "Reduce Motion keeps the trace still");
    ui.click(dictation_ui::CANCEL);
    ui.allow_motion();
    ui.settle(Duration::from_millis(20));
    assert_eq!(ui.play(Duration::from_millis(200)), 0, "cancellation leaves no animation behind");
}
