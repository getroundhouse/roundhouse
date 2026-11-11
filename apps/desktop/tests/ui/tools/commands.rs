//! An opened command: cut off at the end of its line until it's clicked, and what it printed a line at a time, which
//! still selects and copies as it was printed.

use gpui_kit::test::{ElementSnapshot, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, TestAppContext, point, px};

use protocol::event::EventBody;

use super::{COMMAND_LINE, bounds, in_full_access, open_a_command};
use crate::support::{Stack, Ui};

/// Far too long for one line of the transcript.
const LONG: &str = "echo trains wait at a stop signal until it clears and the depot says the line ahead is free of \
                    other trains so they may pass at caution or at clear as the signal shows them";

#[gpui_kit::test]
fn a_command_too_long_for_its_line_shows_whole_when_clicked_and_is_cut_off_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
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
    let mut ui = in_full_access(cx, &stack);
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
    let mut ui = in_full_access(cx, &stack);
    let (command, _) = open_a_command(&mut ui, "echo short");

    ui.click(command);

    assert_eq!(whole(&mut ui, "echo short"), Some(false), "there's nothing more of it to show");
    assert!(ui.shown("no-output/").is_empty(), "and it printed something, so nothing says it didn't");
}

#[gpui_kit::test]
fn what_a_command_printed_selects_across_its_lines_and_copies_as_it_was_printed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
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
fn a_command_that_printed_nothing_says_so_once_it_is_done(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_full_access(cx, &stack);
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
