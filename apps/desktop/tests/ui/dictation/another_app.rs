//! Another app coming in front while Space is held: held to dictate, it goes on until Space is let go of, which the
//! system says where the app may monitor input. Otherwise it stops as letting go does, and held Space types nothing.

use std::time::Duration;

use gpui_kit::TestAppContext;

use super::holding_space::{hold_for, hold_on_to_dictate, hold_space_to_listen};
use super::support::{dictating, hearing, let_go, live, on_the_keyboard, press_space, ready, repeat_space, say};
use crate::support::{Stack, Ui};

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
fn another_app_in_front_for_a_moment_while_space_is_held_goes_on_dictating(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.bring_to_front();
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Please");
    hold_space_to_listen(&mut ui);
    hold_on_to_dictate(&mut ui);
    say(&mut ui, &engine, "fix the flaky test");
    hearing(&mut ui, "fix the");

    // The system shows something in front, such as while it checks an app that's opening, and Space stays down.
    on_the_keyboard(&mut ui, true);
    ui.send_to_back();
    hold_for(&mut ui, Duration::from_millis(200));
    ui.bring_to_front();

    for _ in 0..4 {
        assert!(repeat_space(&mut ui), "the repeats of Space still held type nothing");
    }
    say(&mut ui, &engine, "in hub");
    hearing(&mut ui, "fix the flaky test in");
    on_the_keyboard(&mut ui, false);
    let_go(&mut ui);
    ui.wait("the final text", |ui| ui.composer_text() == "Please Fix the flaky test in hub.");
    press_space(&mut ui);
    assert_eq!(ui.composer_text(), "Please Fix the flaky test in hub. ", "Space pressed again types");
}

#[gpui_kit::test]
fn without_leave_to_monitor_input_another_app_in_front_stops_dictating_and_held_space_types_nothing(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.bring_to_front();
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Please");
    hold_space_to_listen(&mut ui);
    hold_on_to_dictate(&mut ui);
    say(&mut ui, &engine, "fix the flaky test");
    hearing(&mut ui, "fix the");

    // The system won't say whether Space is down, as a Mac won't to an app not let monitor input.
    ui.cx.update(dictation_ui::hide_the_keyboard);
    ui.send_to_back();
    ui.bring_to_front();

    ui.wait("the final text", |ui| ui.composer_text() == "Please Fix the flaky test.");
    assert!(!dictating(&mut ui), "it stops, since it can't tell when Space is let go of");
    for _ in 0..4 {
        assert!(repeat_space(&mut ui), "the repeats of Space still held type nothing");
    }
    let_go(&mut ui);
    press_space(&mut ui);
    assert_eq!(ui.composer_text(), "Please Fix the flaky test. ", "Space pressed again types");
}

#[gpui_kit::test]
fn the_system_is_asked_to_let_the_app_monitor_input_once_space_held_has_dictated(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    let asked = |ui: &mut Ui| ui.cx.read(dictation_ui::asked_to_see_the_keyboard);

    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    ui.click(dictation_ui::STOP);
    ui.wait("the final text", |ui| ui.composer_text() == "Fix the test.");
    assert!(!asked(&mut ui), "the microphone button needs no Space held");

    hold_space_to_listen(&mut ui);
    hold_on_to_dictate(&mut ui);
    say(&mut ui, &engine, "in hub");
    hearing(&mut ui, "in");
    assert!(!asked(&mut ui), "the system's question would stop what's being dictated");
    let_go(&mut ui);
    ui.wait("the final text", |ui| ui.composer_text() == "Fix the test. In hub.");
    assert!(asked(&mut ui));
}

#[gpui_kit::test]
fn space_let_go_of_while_another_app_is_in_front_stops_dictating_as_letting_go_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.bring_to_front();
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.input("Please");
    hold_space_to_listen(&mut ui);
    hold_on_to_dictate(&mut ui);
    on_the_keyboard(&mut ui, true);
    ui.send_to_back();
    say(&mut ui, &engine, "fix the flaky test");
    hearing(&mut ui, "fix the");

    // Space is let go of in the other app, which this one never hears, so it asks the system.
    on_the_keyboard(&mut ui, false);
    hold_for(&mut ui, Duration::from_millis(50));

    ui.wait("the final text", |ui| ui.composer_text() == "Please Fix the flaky test.");
    assert!(!dictating(&mut ui));
    ui.bring_to_front();
    press_space(&mut ui);
    assert_eq!(ui.composer_text(), "Please Fix the flaky test. ", "Space types as it always has");
}

#[gpui_kit::test]
fn another_app_in_front_while_space_is_held_towards_dictating_still_dictates(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.bring_to_front();
    ui.new_workspace();
    ready(&mut ui);
    ui.input("Please");

    hold_space_to_listen(&mut ui);
    on_the_keyboard(&mut ui, true);
    ui.send_to_back();
    ui.bring_to_front();
    hold_on_to_dictate(&mut ui);

    assert!(dictating(&mut ui));
    assert_eq!(ui.composer_text(), "Please", "the space it typed is taken back out");
    assert!(repeat_space(&mut ui), "Space types nothing while it's held to dictate");
}
