//! An opened command: whole, wrapping, and selectable, to copy and run elsewhere, with a button pinned at its top right
//! that copies all of it, and a long one scrolling by itself. What it printed is drawn a line at a time, and still
//! selects and copies as it was printed.

use gpui_kit::base::TextSelection;
use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, ElementId, InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollDelta, TestAppContext, point, px,
};

use protocol::event::EventBody;

use super::{COMMAND_LINE, bounds, in_a_new_workspace, open_a_command};
use crate::support::{Stack, Ui};

/// Far too long for one line of the transcript.
const LONG: &str = "echo trains wait at a stop signal until it clears and the depot says the line ahead is free of \
                    other trains so they may pass at caution or at clear as the signal shows them";

#[gpui_kit::test]
fn a_command_too_long_for_its_line_shows_whole_and_copies_as_it_was_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, _) = open_a_command(&mut ui, LONG);
    let shown = bounds(&mut ui, text(&command));
    // From the start of its first line to past the end of its last.
    let (from, to) = (shown.origin + point(px(1.), px(4.)), shown.bottom_right() - point(px(1.), px(4.)));

    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some(LONG), "every row it wraps onto");
}

#[gpui_kit::test]
fn a_command_of_several_lines_shows_and_copies_every_line(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let script = "echo one\necho two";
    let (command, _) = open_a_command(&mut ui, script);
    let shown = bounds(&mut ui, text(&command));
    let (from, to) = (shown.origin + point(px(1.), px(4.)), shown.bottom_right() - point(px(1.), px(4.)));

    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some(script), "its lines as they were written");
}

#[gpui_kit::test]
fn clicking_a_command_leaves_it_as_it_is_and_a_double_click_selects_a_word(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, _) = open_a_command(&mut ui, LONG);
    let before = bounds(&mut ui, command.clone());

    ui.click(text(&command));

    assert_eq!(bounds(&mut ui, command.clone()), before, "nothing about it changes");

    // On its first row.
    let twice = ClickOptions::new().with_offset(point(px(4.), px(6.))).with_count(2);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_with_options(text(&command), twice, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("echo"), "the word under the pointer");
}

#[gpui_kit::test]
fn a_command_s_copy_button_shows_while_it_s_hovered_and_copies_all_of_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, output) = open_a_command(&mut ui, LONG);
    ui.hover(output);
    assert!(!ui.visible(copy(&command)), "hidden while what it printed is hovered");

    ui.hover(text(&command));
    assert!(ui.visible(copy(&command)), "shown while the command is");

    ui.click(copy(&command));

    assert_eq!(copied(&mut ui).as_deref(), Some(LONG));
}

#[gpui_kit::test]
fn a_long_command_scrolls_by_itself_under_its_copy_button(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let script = (1..=30).map(|n| format!("echo signal {n}")).collect::<Vec<_>>().join("\n");
    let (command, _) = open_a_command(&mut ui, &script);
    let (held, shown) = (bounds(&mut ui, area(&command)), bounds(&mut ui, text(&command)));
    assert_eq!(held.size.height, px(118.), "half as tall as what it printed can be");
    ui.hover(area(&command));
    let button = bounds(&mut ui, copy(&command));
    assert!(button.top() < held.top() + px(20.), "at its top");
    assert!(shown.right() <= button.left(), "and the text stops short of it");

    ui.scroll(area(&command), ScrollDelta::Pixels(point(px(0.), px(-60.))));

    assert!(bounds(&mut ui, text(&command)).top() < shown.top() - px(30.), "the command scrolled up");
    assert_eq!(bounds(&mut ui, copy(&command)), button, "and the button stayed where it was");
}

#[gpui_kit::test]
fn what_a_command_printed_selects_across_its_lines_and_copies_as_it_was_printed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, r"printf 'one\n\nthree\nfour\n'");
    assert!(ui.has(ElementId::from((output.clone(), "lines"))), "drawn a line at a time, rather than as one text");
    let printed = bounds(&mut ui, output);
    // From the start of its first line to past the end of its last, inside the space around them.
    let (from, to) = (printed.origin + point(px(14.), px(12.)), printed.bottom_right() - point(px(14.), px(12.)));

    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("one\n\nthree\nfour"), "with its blank line");
}

#[gpui_kit::test]
fn what_a_command_printed_shows_what_s_selected_while_it_s_dragged_over(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, r"printf 'one\ntwo\nthree\n'");
    let printed = bounds(&mut ui, output);
    let (from, to) = (printed.origin + point(px(14.), px(12.)), printed.bottom_right() - point(px(14.), px(12.)));

    assert_eq!(selected_while_dragged(&mut ui, from, to), "one\ntwo\nthree");
}

#[gpui_kit::test]
fn a_command_that_printed_nothing_says_so_once_it_is_done(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    ui.input("tool:true; say:done");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("done"));
    ui.wait("the turn's end", |ui| ui.events().iter().any(|e| matches!(e.body, EventBody::TurnEnded { .. })));

    ui.click_labelled(COMMAND_LINE, 0);

    assert!(ui.shown("output/").is_empty(), "it printed nothing to show");
    assert_eq!(ui.shown("no-output/").len(), 1, "so it says so");
}

/// The command's text, which is for selecting.
fn text(command: &ElementId) -> ElementId {
    ElementId::from((command.clone(), "text"))
}

/// The button that copies the command.
fn copy(command: &ElementId) -> ElementId {
    ElementId::from((command.clone(), "copy"))
}

/// What the command scrolls in once it's too tall, with its copy button.
fn area(command: &ElementId) -> ElementId {
    ElementId::from((command.clone(), "area"))
}

/// What was last copied.
fn copied(ui: &mut Ui<'_>) -> Option<String> {
    ui.cx.read_from_clipboard().and_then(|item| item.text())
}

/// What's selected with the button still down, having pressed at `from` and dragged to `to`, as the app gets a drag:
/// nothing redraws the window between its events but what they change. The kit's own pointer helpers draw it again,
/// and so would hide a text that doesn't draw again when what's selected of it changes.
fn selected_while_dragged(ui: &mut Ui<'_>, from: Point<Pixels>, to: Point<Pixels>) -> String {
    let (pressed, left) = (Some(MouseButton::Left), MouseButton::Left);
    let modifiers = Modifiers::default();
    send(ui, MouseMoveEvent { position: from, pressed_button: None, modifiers });
    send(ui, MouseDownEvent { button: left, position: from, modifiers, click_count: 1, first_mouse: false });
    send(ui, MouseMoveEvent { position: (from + to) / 2., pressed_button: pressed, modifiers });
    send(ui, MouseMoveEvent { position: to, pressed_button: pressed, modifiers });
    let selected = ui.cx.update_window(ui.handle, |_, window, cx| TextSelection::selected_text(window, cx)).unwrap();
    send(ui, MouseUpEvent { button: left, position: to, modifiers, click_count: 1 });
    selected
}

/// Sends `event` to the window, and lets the app draw what it changed.
fn send(ui: &mut Ui<'_>, event: impl InputEvent) {
    let event = event.to_platform_input();
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event, cx)).unwrap();
    ui.frame();
}
