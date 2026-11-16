//! Holding `Option-Space` to dictate: what's heard while it's held, letting go of it wherever focus is, Esc with Option
//! still held, Space's repeats, and `Option-Space` typing as it did where there's nothing to dictate to.

use gpui_kit::TestAppContext;

use super::support::{ended, hearing, hold_option_space, kept, let_go, live, ready, say, sent, type_keys};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn holding_the_key_shows_what_is_heard_and_letting_go_puts_it_in_the_draft_without_sending(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    let engine = ready(&mut ui);
    ui.input("Please");

    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the flaky test");
    hearing(&mut ui, "fix the");
    assert_eq!(ui.composer_text(), "Please", "live text is drawn, not typed");
    let_go(&mut ui);

    ui.wait("the final text", |ui| ui.composer_text() == "Please Fix the flaky test.");
    ended(&mut ui);
    ui.wait("the hub to keep it as the draft", |_| kept(&stack, &chat) == "Please Fix the flaky test.");
    assert!(!sent(&ui), "dictation never sends");
}

#[gpui_kit::test]
fn each_sentence_goes_in_once_its_final_while_the_key_is_still_held(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);

    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test. / in hub");

    ui.wait("the first sentence", |ui| ui.composer_text() == "Fix the test.");
    hearing(&mut ui, "in");
    let_go(&mut ui);
    ui.wait("the second sentence", |ui| ui.composer_text() == "Fix the test. In hub.");
}

#[gpui_kit::test]
fn escape_cancels_and_puts_the_prompt_back_as_it_was_with_option_still_held_or_not(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Keep this");

    // Option is still down, held for the key, so Escape comes with it.
    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test. / in hub");
    ui.wait("the first sentence", |ui| ui.composer_text() == "Keep this Fix the test.");
    ui.press("alt-escape");
    assert_eq!(ui.composer_text(), "Keep this");
    assert_eq!(live(&mut ui), None);
    let_go(&mut ui);

    ui.click("dictate");
    say(&mut ui, &engine, "fix the test. / in hub");
    ui.wait("the first sentence", |ui| ui.composer_text() == "Keep this Fix the test.");
    ui.press("escape");
    assert_eq!(ui.composer_text(), "Keep this");

    // Dictating works again after.
    ui.click("dictate");
    say(&mut ui, &engine, "in hub");
    ui.click(dictation_ui::STOP);
    ui.wait("the final text", |ui| ui.composer_text() == "Keep this In hub.");
}

#[gpui_kit::test]
fn space_repeating_after_option_is_let_go_types_nothing_until_its_let_go(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test");

    // Option let go of first: Space goes on repeating on its own.
    for _ in 0..3 {
        type_keys(&mut ui, "space", " ");
    }
    let_go(&mut ui);
    ui.wait("the final text", |ui| ui.composer_text() == "Fix the test.");

    type_keys(&mut ui, "space", " ");
    assert_eq!(ui.composer_text(), "Fix the test. ", "Space types again once it's let go of");
}

#[gpui_kit::test]
fn letting_go_of_the_key_in_a_dialog_stops_it_and_space_types_there_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    // A dialog the kit draws beside the app's view, with the focus, opened while the key is held.
    ui.press("secondary-n");
    ui.wait("the dialog", Ui::has_dialog);
    let_go(&mut ui);

    ui.wait("the final text, with the dialog still open", |ui| ui.composer_text() == "Fix the test.");
    type_keys(&mut ui, "space", " ");
    assert_eq!(ui.dialog_text(), " ", "Space types in the dialog once it's let go of");
}

#[gpui_kit::test]
fn the_key_types_as_it_did_where_there_is_nothing_to_dictate_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();

    type_keys(&mut ui, "alt-space", "\u{a0}");

    assert_eq!(ui.composer_text(), "\u{a0}", "Option-Space types a non-breaking space until dictation is ready");
}
