//! An opened command: cut off at the end of its line until it's clicked, and what it printed a line at a time, which
//! still selects and copies as it was printed, and shows what's selected as it's dragged over.

use gpui_kit::base::TextSelection;
use gpui_kit::test::{ElementSnapshot, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, ElementId, InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, TestAppContext, point, px,
};

use protocol::event::EventBody;

use super::{COMMAND_LINE, bounds, in_a_new_workspace, open_a_command};
use crate::support::{Stack, Ui};

/// Far too long for one line of the transcript.
const LONG: &str = "echo trains wait at a stop signal until it clears and the depot says the line ahead is free of \
                    other trains so they may pass at caution or at clear as the signal shows them";

#[gpui_kit::test]
fn a_command_too_long_for_its_line_shows_whole_when_clicked_and_is_cut_off_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, _) = open_a_command(&mut ui, LONG);
    let one_line = bounds(&mut ui, command.clone()).size.height;
    assert_eq!(whole(&mut ui, LONG), Some(false), "a command starts cut off");

    ui.click(command.clone());

    assert_eq!(whole(&mut ui, LONG), Some(true));
    assert!(bounds(&mut ui, command.clone()).size.height > one_line, "whole, it wraps onto more lines");

    ui.click(command.clone());

    assert_eq!(whole(&mut ui, LONG), Some(false));
    assert_eq!(bounds(&mut ui, command.clone()).size.height, one_line);
}

#[gpui_kit::test]
fn a_command_of_several_lines_shows_its_first_until_it_is_clicked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let script = "echo one\necho two";
    let (command, _) = open_a_command(&mut ui, script);
    let first_line = bounds(&mut ui, command.clone()).size.height;

    ui.click(command.clone());

    assert_eq!(whole(&mut ui, script), Some(true), "its first line fits, but there's more of it");
    assert!(bounds(&mut ui, command.clone()).size.height > first_line);

    ui.click(command.clone());

    assert_eq!(whole(&mut ui, script), Some(false));
    assert_eq!(bounds(&mut ui, command).size.height, first_line, "back to its first line");
}

#[gpui_kit::test]
fn a_command_that_fits_its_line_stays_as_it_is_when_clicked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    let (command, _) = open_a_command(&mut ui, "echo short");

    ui.click(command);

    assert_eq!(whole(&mut ui, "echo short"), Some(false), "there's nothing more of it to show");
    assert!(ui.shown("no-output/").is_empty(), "and it printed something, so nothing says it didn't");
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

    let copied = ui.cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("one\n\nthree\nfour"), "with its blank line");
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

/// Whether the command labelled `command` says it's shown whole, as it tells assistive apps.
fn whole(ui: &mut Ui<'_>, command: &str) -> Option<bool> {
    ui.labelled(command).first().and_then(ElementSnapshot::expanded)
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
