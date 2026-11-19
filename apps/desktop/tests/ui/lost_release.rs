//! A press whose release the system keeps, as Windows does once a press on the window's frame resizes or moves the
//! window. These can't run on Windows, so they stand in for it: a press, then a move that says no button is held, and
//! the system saying whether one is. What a press, a move and a release do is the app's own, so these open on a fake
//! hub, holding two workspaces, each with a chat.
//!
//! What waits for a release is what's let down when it's lost. A selection in a chat's text no longer does: the
//! transcript's own, and the kit's since its v0.10.0, end at a move that says the button isn't held, so a lost release
//! can't leave one following the pointer. A row in the sidebar still does, since it's held pressed until it's let go.

#![cfg(test)]

use client_core::fake_hub::World;
use desktop::lost_release::{self, Button};
use gpui_kit::{ElementId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, SharedString, TestAppContext};
use gpui_kit::{point, px};
use protocol::model::WorkspaceId;

use crate::support::{FakeHub, Ui, open_on_the_world, sidebar_row, with_a_chat};

/// The window on a fake hub holding two workspaces, each with a chat, showing the second. Gives the first, which
/// isn't showing. The hub goes with the test, so it's returned to be kept.
fn in_a_workspace(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, WorkspaceId) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let [other, showing] = ["ledger", "points"].map(|name| with_a_chat(&mut world, &repo, name));
    let (fake, mut ui) = open_on_the_world(cx, world);
    ui.click(sidebar_row(&showing));
    ui.wait("the workspace", |ui| ui.view().selected() == Some(&showing) && ui.transcript().is_some());
    (fake, ui, other)
}

/// A row held pressed keeps its archive button, though the pointer has left it, until the press is let go, as
/// `sidebar.rs` has it. On Windows, a press on the window's frame over a row, to resize the window, never was, so the
/// button stayed on a row the pointer had long left.
#[gpui_kit::test]
fn a_rows_press_whose_release_was_lost_is_let_go_at_the_next_move(cx: &mut TestAppContext) {
    let (_fake, mut ui, pressed) = in_a_workspace(cx);
    let button = ElementId::Name(SharedString::from(format!("archive-{pressed}")));
    ui.hover(sidebar_row(&pressed));
    assert!(ui.has(button.clone()), "the pointer is over the row");
    let on_row = ui.bounds(sidebar_row(&pressed)).center();
    let elsewhere = ui.bounds("workspace-title").center();
    ui.cx.update(|cx| lost_release::the_left_button_is(Button::Up, cx));
    ui.send(MouseDownEvent { button: MouseButton::Left, position: on_row, click_count: 1, ..Default::default() });

    ui.send(MouseMoveEvent { position: elsewhere, pressed_button: None, ..Default::default() });

    assert!(!ui.has(button), "let go off the row, the pointer has left it");
}

/// On Windows, a press on a button in the strip along the top goes on with moves that say no button is held, once the
/// strip is taken for a title bar, since the press was on what Windows calls the frame.
#[gpui_kit::test]
fn a_press_held_while_moves_say_no_button_is_held_is_let_go_only_when_it_is_released(cx: &mut TestAppContext) {
    let (_fake, mut ui, _) = in_a_workspace(cx);
    assert!(ui.changes_open(), "the sidebar of changes starts open");
    let on = ui.bounds("toggle-diff").center();
    let beside = on + point(px(1.), px(0.));
    ui.cx.update(|cx| lost_release::the_left_button_is(Button::Down, cx));
    ui.send(MouseMoveEvent { position: on, pressed_button: None, ..Default::default() });
    ui.send(MouseDownEvent { button: MouseButton::Left, position: on, click_count: 1, ..Default::default() });

    ui.send(MouseMoveEvent { position: beside, pressed_button: None, ..Default::default() });

    assert!(ui.changes_open(), "the button is still held, so it isn't clicked yet");

    ui.send(MouseUpEvent { button: MouseButton::Left, position: beside, click_count: 1, ..Default::default() });

    assert!(!ui.changes_open(), "it's clicked once it's let go");
}
