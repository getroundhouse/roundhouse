//! A press whose release the system keeps, as Windows does once a press on the window's frame resizes or moves the
//! window. These can't run on Windows, so they stand in for it: a press, then a move that says no button is held, and
//! the system saying whether one is. What a press, a move and a release do is the app's own, so these open on a fake
//! hub, holding a workspace whose chat ran a command.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{command, finished_turn, printed, taken, user_prompt};
use desktop::lost_release::{self, Button};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext, point, px};
use protocol::model::{Seq, SessionStatus};

use crate::support::{FakeHub, Ui, open_on_the_world};
use crate::tools::COMMAND_LINE;

/// The window on a fake hub holding a workspace whose chat ran a command that printed four short lines. The hub goes
/// with the test, so it's returned to be kept.
fn in_a_workspace(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let events = vec![
        user_prompt("Print four lines"),
        taken(Seq::new(1)),
        command("msg-1", "toolu-1", r"printf 'one\n\nthree\nfour\n'", None),
        printed("toolu-1", "one\n\nthree\nfour\n"),
        finished_turn(4, 0.01),
    ];
    world.chat(&workspace, SessionStatus::Idle, events);
    let (fake, mut ui) = open_on_the_world(cx, world);
    ui.wait("the command's line", |ui| !ui.labelled(COMMAND_LINE).is_empty());
    (fake, ui)
}

/// Moving the pointer over a chat's text went on selecting it, with no button held, until the next click. On Windows
/// the press is on the window's frame. Here it's on the text, so what a move would select is known.
#[gpui_kit::test]
fn a_move_after_a_press_whose_release_was_lost_selects_nothing(cx: &mut TestAppContext) {
    let (_fake, mut ui) = in_a_workspace(cx);
    ui.click_labelled(COMMAND_LINE, 0);
    ui.wait("what the command printed", |ui| ui.shown("output/").len() == 1);
    let output = ui.shown("output/").remove(0);
    let printed = ui.bounds(output);
    // As far apart in what it printed as the test that selects it by dragging.
    let (from, to) = (printed.origin + point(px(14.), px(12.)), printed.bottom_right() - point(px(14.), px(12.)));
    ui.cx.update(|cx| lost_release::the_left_button_is(Button::Up, cx));
    ui.send(MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() });

    ui.send(MouseMoveEvent { position: to, pressed_button: None, ..Default::default() });
    ui.press("secondary-c");

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, None, "nothing is selected to copy");
}

/// On Windows, a press on a button in the strip along the top goes on with moves that say no button is held, once the
/// strip is taken for a title bar, since the press was on what Windows calls the frame.
#[gpui_kit::test]
fn a_press_held_while_moves_say_no_button_is_held_is_let_go_only_when_it_is_released(cx: &mut TestAppContext) {
    let (_fake, mut ui) = in_a_workspace(cx);
    assert!(ui.changes_open(), "the sidebar of changes starts open");
    let on = ui.cx.update_window(ui.handle, |_, window, _| window.find("toggle-diff").bounds().center()).unwrap();
    let beside = on + point(px(1.), px(0.));
    ui.cx.update(|cx| lost_release::the_left_button_is(Button::Down, cx));
    ui.send(MouseMoveEvent { position: on, pressed_button: None, ..Default::default() });
    ui.send(MouseDownEvent { button: MouseButton::Left, position: on, click_count: 1, ..Default::default() });

    ui.send(MouseMoveEvent { position: beside, pressed_button: None, ..Default::default() });

    assert!(ui.changes_open(), "the button is still held, so it isn't clicked yet");

    ui.send(MouseUpEvent { button: MouseButton::Left, position: beside, click_count: 1, ..Default::default() });

    assert!(!ui.changes_open(), "it's clicked once it's let go");
}
