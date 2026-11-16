//! Holding plain Space in the composer to dictate: it types a space, hears what's said without showing it from
//! `SPACE_LISTENS_AFTER`, and dictates from `SPACE_DICTATES_AFTER`, without the space and with what it heard. Letting
//! go, or another key, before then leaves the space and puts in nothing it heard, and where there's nothing to dictate
//! to, Space repeats as it always has.

use std::time::Duration;

use desktop::workspace_pane::{SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER};
use gpui_kit::TestAppContext;
use protocol::model::SessionStatus;

use gpui_kit::{AppContext as _, SharedString};

use super::support::{
    dictating, hearing, hold_option_space, let_go, live, press_space, ready, repeat_space, say, type_keys,
};
use crate::support::{Stack, Ui, id};

/// A moment, less than either step of holding Space.
const A_MOMENT: Duration = Duration::from_millis(1);

/// How long Space goes on being held, once it listens, before it dictates.
const LISTENING_UNSEEN: Duration = SPACE_DICTATES_AFTER.checked_sub(SPACE_LISTENS_AFTER).unwrap();

/// Lets `time` go by while Space is held.
fn hold_for(ui: &mut Ui, time: Duration) {
    ui.cx.executor().advance_clock(time);
    ui.frame();
}

/// Presses Space and holds it until it listens.
fn hold_space_to_listen(ui: &mut Ui) {
    press_space(ui);
    hold_for(ui, SPACE_LISTENS_AFTER);
}

/// Goes on holding Space, which has listened, until it dictates.
fn hold_on_to_dictate(ui: &mut Ui) {
    hold_for(ui, LISTENING_UNSEEN);
}

#[gpui_kit::test]
fn holding_space_dictates_what_was_said_once_it_listened_without_the_space_it_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Please");

    press_space(&mut ui);
    assert_eq!(ui.composer_text(), "Please ", "Space types as it always has");
    hold_for(&mut ui, SPACE_LISTENS_AFTER.checked_sub(A_MOMENT).unwrap());
    say(&mut ui, &engine, "in hub");
    hold_for(&mut ui, A_MOMENT);
    say(&mut ui, &engine, "fix the flaky test");
    hold_for(&mut ui, LISTENING_UNSEEN.checked_sub(A_MOMENT).unwrap());
    assert!(repeat_space(&mut ui), "Space's repeats type nothing while it's held towards dictating");
    assert_eq!(live(&mut ui), None, "what's heard shows only once it dictates");
    assert!(!dictating(&mut ui));
    assert_eq!(ui.composer_text(), "Please ");

    hold_for(&mut ui, A_MOMENT);
    assert!(dictating(&mut ui));
    assert_eq!(ui.composer_text(), "Please", "the space it typed is taken back out");
    assert!(repeat_space(&mut ui), "Space types nothing while it's held to dictate");
    hearing(&mut ui, "fix the");
    let_go(&mut ui);
    ui.wait("the final text", |ui| ui.composer_text() == "Please Fix the flaky test.");
}

#[gpui_kit::test]
fn escape_puts_back_the_prompt_as_it_was_before_space(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Keep this");

    hold_space_to_listen(&mut ui);
    hold_on_to_dictate(&mut ui);
    say(&mut ui, &engine, "fix the test. / in hub");
    ui.wait("the first sentence", |ui| ui.composer_text() == "Keep this Fix the test.");
    ui.press("escape");

    assert_eq!(ui.composer_text(), "Keep this");
    let_go(&mut ui);
    press_space(&mut ui);
    assert_eq!(ui.composer_text(), "Keep this ", "Space types again once it's let go of");
}

#[gpui_kit::test]
fn letting_go_of_space_before_it_dictates_leaves_the_space_and_nothing_it_heard(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Please");

    hold_space_to_listen(&mut ui);
    say(&mut ui, &engine, "fix the test. / in hub");
    let_go(&mut ui);
    hold_on_to_dictate(&mut ui);

    assert_eq!(ui.composer_text(), "Please ");
    assert_eq!(live(&mut ui), None);
    assert!(!dictating(&mut ui));
    // Pressed again, it starts again from nothing.
    hold_space_to_listen(&mut ui);
    hold_for(&mut ui, LISTENING_UNSEEN.checked_sub(A_MOMENT).unwrap());
    assert!(!dictating(&mut ui));
    assert_eq!(ui.composer_text(), "Please  ");
}

#[gpui_kit::test]
fn another_key_while_space_is_held_types_and_dictates_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    ui.input("Please");

    hold_space_to_listen(&mut ui);
    type_keys(&mut ui, "a", "a");
    hold_on_to_dictate(&mut ui);

    assert_eq!(ui.composer_text(), "Please a");
    assert_eq!(live(&mut ui), None);
    assert!(!repeat_space(&mut ui), "a repeat that comes after another key isn't Space held to dictate");
    assert!(!dictating(&mut ui));
}

#[gpui_kit::test]
fn a_key_a_binding_takes_while_space_is_held_forgets_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    ui.input("Please");

    hold_space_to_listen(&mut ui);
    ui.press("backspace");
    hold_on_to_dictate(&mut ui);

    assert_eq!(ui.composer_text(), "Please");
    assert_eq!(live(&mut ui), None);
    assert!(!dictating(&mut ui));
}

#[gpui_kit::test]
fn option_pressed_while_space_is_held_dictates_at_once_and_escape_leaves_nothing_held(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);

    hold_space_to_listen(&mut ui);
    hold_option_space(&mut ui);
    assert!(dictating(&mut ui));
    ui.press("alt-escape");
    hold_on_to_dictate(&mut ui);

    assert_eq!(live(&mut ui), None, "Space held doesn't dictate again once Option has");
    assert!(!dictating(&mut ui));
}

#[gpui_kit::test]
fn the_microphone_clicked_while_space_is_held_dictates_and_space_types_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    ui.input("Please");

    hold_space_to_listen(&mut ui);
    ui.click("dictate");
    hold_on_to_dictate(&mut ui);
    for _ in 0..4 {
        assert!(!repeat_space(&mut ui), "Space types while the microphone dictates");
    }

    assert_eq!(ui.composer_text(), "Please ", "what the microphone dictates into is left as it was");
    assert!(dictating(&mut ui));
}

#[gpui_kit::test]
fn another_app_in_front_while_space_is_held_forgets_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.bring_to_front();
    ui.new_workspace();
    ready(&mut ui);

    hold_space_to_listen(&mut ui);
    // Space is let go of in the other app, which this one never hears.
    ui.send_to_back();
    hold_on_to_dictate(&mut ui);

    assert_eq!(live(&mut ui), None);
    assert!(!dictating(&mut ui));
    ui.bring_to_front();
    assert!(!repeat_space(&mut ui), "a repeat after coming back isn't Space held to dictate");
}

#[gpui_kit::test]
fn what_changes_the_prompt_while_space_is_held_stays_with_the_space(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    ui.input("Please");

    hold_space_to_listen(&mut ui);
    // Not a key, as a paste from the Edit menu isn't.
    let pane = ui.pane().unwrap();
    let composer = ui.cx.read_entity(&pane, |pane, _| pane.composer.clone());
    ui.cx
        .update_window(ui.handle, |_, window, cx| composer.update(cx, |c, cx| c.insert("pasted", window, cx)))
        .unwrap();
    hold_on_to_dictate(&mut ui);

    assert!(dictating(&mut ui));
    assert_eq!(ui.composer_text(), "Please pasted", "the space stays, and so does what changed");
}

#[gpui_kit::test]
fn space_held_on_the_composers_menu_rather_than_where_the_prompt_is_typed_doesnt_dictate(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);

    ui.click(SharedString::from("add"));
    hold_space_to_listen(&mut ui);
    hold_on_to_dictate(&mut ui);

    assert!(!dictating(&mut ui));
    assert_eq!(live(&mut ui), None);
}

#[gpui_kit::test]
fn a_question_taking_the_composers_place_forgets_space_held_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ready(&mut ui);
    hold_space_to_listen(&mut ui);

    // From another client, while Space is still held.
    stack.client().session_prompt(&chat, "ask:Red or blue?=Red/Blue", None).unwrap();
    ui.wait("the agent to ask", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.next_open_approval();
    ui.wait("what it asks to show", |ui| ui.has(id("question")));
    ui.click(id("question-answer-0"));
    ui.click(id("question-send"));
    ui.wait_for_the_turn();
    hold_on_to_dictate(&mut ui);

    assert_eq!(live(&mut ui), None, "Space held where the question was doesn't dictate into the composer back");
    assert!(!dictating(&mut ui));
}

#[gpui_kit::test]
fn space_repeats_as_it_always_has_where_there_is_nothing_to_dictate_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();

    hold_space_to_listen(&mut ui);
    for _ in 0..6 {
        assert!(!repeat_space(&mut ui), "each repeat types a space, as the system has it");
    }
    hold_on_to_dictate(&mut ui);
    assert!(!dictating(&mut ui));
}

#[gpui_kit::test]
fn space_types_as_it_always_has_while_the_microphone_dictates(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ready(&mut ui);
    ui.click("dictate");

    press_space(&mut ui);
    for _ in 0..6 {
        assert!(!repeat_space(&mut ui));
    }
    assert_eq!(ui.composer_text(), " ");
    let_go(&mut ui);
    assert!(dictating(&mut ui), "letting go of Space doesn't stop what the microphone started");
}
