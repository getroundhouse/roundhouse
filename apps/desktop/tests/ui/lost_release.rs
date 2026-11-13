//! A press whose release the system keeps, as Windows does once a press on the window's frame resizes or moves the
//! window. These can't run on Windows, so they stand in for it: a press, then a move that says no button is held, and
//! the system saying whether one is.

#![cfg(test)]

use desktop::lost_release::{self, Button};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, InputEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext, point, px,
};

use crate::support::{Stack, Ui};
use crate::tools::{in_a_new_workspace, open_a_command};

/// Moving the pointer over a chat's text went on selecting it, with no button held, until the next click. On Windows
/// the press is on the window's frame. Here it's on the text, so what a move would select is known.
#[gpui_kit::test]
fn a_move_after_a_press_whose_release_was_lost_selects_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, r"printf 'one\n\nthree\nfour\n'");
    let printed = ui.bounds(output);
    // As far apart in what it printed as the test that selects it by dragging.
    let (from, to) = (printed.origin + point(px(14.), px(12.)), printed.bottom_right() - point(px(14.), px(12.)));
    ui.cx.update(|cx| lost_release::the_left_button_is(Button::Up, cx));
    send(&mut ui, MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() });

    send(&mut ui, MouseMoveEvent { position: to, pressed_button: None, ..Default::default() });
    ui.press("secondary-c");

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, None, "nothing is selected to copy");
}

/// On Windows, a press on a button in the strip along the top goes on with moves that say no button is held, once the
/// strip is taken for a title bar, since the press was on what Windows calls the frame.
#[gpui_kit::test]
fn a_press_held_while_moves_say_no_button_is_held_is_let_go_only_when_it_is_released(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    assert!(ui.changes_open(), "the sidebar of changes starts open");
    let on = ui.cx.update_window(ui.handle, |_, window, _| window.find("toggle-diff").bounds().center()).unwrap();
    let beside = on + point(px(1.), px(0.));
    ui.cx.update(|cx| lost_release::the_left_button_is(Button::Down, cx));
    send(&mut ui, MouseMoveEvent { position: on, pressed_button: None, ..Default::default() });
    send(&mut ui, MouseDownEvent { button: MouseButton::Left, position: on, click_count: 1, ..Default::default() });

    send(&mut ui, MouseMoveEvent { position: beside, pressed_button: None, ..Default::default() });

    assert!(ui.changes_open(), "the button is still held, so it isn't clicked yet");

    send(&mut ui, MouseUpEvent { button: MouseButton::Left, position: beside, click_count: 1, ..Default::default() });

    assert!(!ui.changes_open(), "it's clicked once it's let go");
}

/// Sends the window a pointer event, and lets it draw.
fn send(ui: &mut Ui<'_>, event: impl InputEvent) {
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event.to_platform_input(), cx)).unwrap();
    ui.frame();
}
