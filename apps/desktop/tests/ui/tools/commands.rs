//! An opened command: whole, wrapping, and selectable, to copy and run elsewhere, with a button pinned at its top right
//! that copies all of it, and a long one scrolling by itself. What it printed is drawn a line at a time, and still
//! selects and copies as it was printed.

use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, ElementId, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollDelta, TestAppContext, point, px,
};

use protocol::event::EventBody;

use super::{COMMAND_LINE, in_a_new_workspace, open_a_command};
use crate::support::{Stack, Ui};

/// Far too long for one line of the transcript.
const LONG: &str = "echo trains wait at a stop signal until it clears and the depot says the line ahead is free of \
                    other trains so they may pass at caution or at clear as the signal shows them";

#[gpui_kit::test]
fn a_command_too_long_for_its_line_shows_whole_and_copies_as_it_was_written(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, _) = open_a_command(&mut ui, LONG);
    let shown = ui.bounds(text(&command));
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
    let shown = ui.bounds(text(&command));
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
    let before = ui.bounds(command.clone());

    ui.click(text(&command));

    assert_eq!(ui.bounds(command.clone()), before, "nothing about it changes");

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
    let (held, shown) = (ui.bounds(area(&command)), ui.bounds(text(&command)));
    assert_eq!(held.size.height, px(118.), "half as tall as what it printed can be");
    ui.hover(area(&command));
    let button = ui.bounds(copy(&command));
    assert!(button.top() < held.top() + px(20.), "at its top");
    assert!(shown.right() <= button.left(), "and the text stops short of it");

    ui.scroll(area(&command), ScrollDelta::Pixels(point(px(0.), px(-60.))));

    assert!(ui.bounds(text(&command)).top() < shown.top() - px(30.), "the command scrolled up");
    assert_eq!(ui.bounds(copy(&command)), button, "and the button stayed where it was");
}

#[gpui_kit::test]
fn what_a_command_printed_selects_across_its_lines_and_copies_as_it_was_printed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, r"printf 'one\n\nthree\nfour\n'");
    assert!(ui.has(ElementId::from((output.clone(), "lines"))), "drawn a line at a time, rather than as one text");
    let printed = ui.bounds(output);
    // From the space the block gives it above and before its first line to the space after its last.
    let (from, to) = (printed.origin + point(px(4.), px(4.)), printed.bottom_right() - point(px(4.), px(4.)));

    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("one\n\nthree\nfour"), "with its blank line");
}

#[gpui_kit::test]
fn what_a_command_printed_is_selected_as_it_s_dragged_over(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, r"printf 'one\ntwo\nthree\n'");
    let (from, to) = across(ui.bounds(output));

    assert_eq!(selected_while_dragged(&mut ui, from, to), "one\ntwo\nthree");
}

/// What's selected of what a command printed stays selected once its row has left the window, and is copied from
/// there.
#[gpui_kit::test]
fn what_a_command_printed_stays_selected_once_its_row_has_left_the_window(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, r"printf 'one\ntwo\nthree\n'");
    let (from, to) = across(ui.bounds(output.clone()));
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();

    for _ in 0..10 {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(400.))));
    }
    assert!(!ui.visible(output), "its row has left the window");
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("one\ntwo\nthree"));
}

/// A triple-click on what a command printed selects the line under it, as one on the agent's words selects a
/// paragraph, however it's drawn: here as one text, since there's too much of it to draw a line at a time.
#[gpui_kit::test]
fn a_triple_click_on_what_a_command_printed_selects_the_line_under_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, "seq -f 'signal %g' 1 250");
    assert!(!ui.has(ElementId::from((output.clone(), "lines"))), "drawn as one text");

    // On its first line, inside the space around it.
    let thrice = ClickOptions::new().with_offset(inside()).with_count(3);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_with_options(output, thrice, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("signal 1"), "the line, not its first word");
}

/// A press in the space a block gives what a command printed, above its first line, is on that line, rather than at
/// the end of the command above it.
#[gpui_kit::test]
fn a_double_click_above_what_a_command_printed_selects_its_first_word(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (_, output) = open_a_command(&mut ui, r"printf 'printed here\n'");

    let twice = ClickOptions::new().with_offset(point(px(14.), px(3.))).with_count(2);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_with_options(output, twice, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("printed"));
}

/// A press on the `$` before a command, or in the space around it, is on the command, so a double-click there selects
/// its first word.
#[gpui_kit::test]
fn a_double_click_on_a_command_s_dollar_selects_its_first_word(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, _) = open_a_command(&mut ui, LONG);
    let dollar = ui.bounds(text(&command)).left_center() - ui.bounds(command.clone()).origin;

    let twice = ClickOptions::new().with_offset(point(dollar.x - px(10.), px(4.))).with_count(2);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_with_options(command, twice, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("echo"));
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

/// From the start of what a command printed, at `printed`, to past the end of its last line, inside the space the block
/// gives it.
fn across(printed: Bounds<Pixels>) -> (Point<Pixels>, Point<Pixels>) {
    (printed.origin + inside(), printed.bottom_right() - inside())
}

/// How far from a corner of what a command printed its first or last line is, inside the space the block gives it.
fn inside() -> Point<Pixels> {
    point(px(14.), px(12.))
}

/// What was last copied.
fn copied(ui: &mut Ui<'_>) -> Option<String> {
    ui.cx.read_from_clipboard().and_then(|item| item.text())
}

/// What's selected with the button still down, having pressed at `from` and dragged to `to`, as the app gets a drag:
/// nothing redraws the window between its events but what they change. The kit's own pointer helpers draw it again,
/// and so would hide a selection that only follows the pointer as the window's drawn.
fn selected_while_dragged(ui: &mut Ui<'_>, from: Point<Pixels>, to: Point<Pixels>) -> String {
    let (pressed, left) = (Some(MouseButton::Left), MouseButton::Left);
    let modifiers = Modifiers::default();
    ui.send(MouseMoveEvent { position: from, pressed_button: None, modifiers });
    ui.send(MouseDownEvent { button: left, position: from, modifiers, click_count: 1, first_mouse: false });
    ui.send(MouseMoveEvent { position: (from + to) / 2., pressed_button: pressed, modifiers });
    ui.send(MouseMoveEvent { position: to, pressed_button: pressed, modifiers });
    let transcript = ui.transcript().expect("the chat");
    let selected = ui.cx.read_entity(&transcript, |t, _| t.as_copied());
    ui.send(MouseUpEvent { button: left, position: to, modifiers, click_count: 1 });
    selected
}
