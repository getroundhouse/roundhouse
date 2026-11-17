//! An opened command: whole, wrapping, and selectable, to copy and run elsewhere, with a button pinned at its top right
//! that copies all of it, and a long one scrolling by itself. What it printed is drawn a line at a time, and still
//! selects and copies as it was printed. Only what the hub recorded matters, so each chat is made up for a fake hub.

use client_core::fake_hub::events;
use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, ClipboardItem, ElementId, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollDelta, TestAppContext, point, px,
};

use super::COMMAND_LINE;
use super::opening::{open_on_a_turn, ran, to_the_end};
use crate::support::{FakeHub, Ui};

/// Far too long for one line of the transcript.
const LONG: &str = "echo trains wait at a stop signal until it clears and the depot says the line ahead is free of \
                    other trains so they may pass at caution or at clear as the signal shows them";

/// One row for a command too long for its line, which shows whole, and one for a command of several lines, which shows
/// every line: each copies as it was written.
#[gpui_kit::test]
fn a_command_shows_whole_and_copies_as_it_was_written(cx: &mut TestAppContext) {
    let script = "echo one\necho two";
    let (_fake, mut ui, opened) = open_on_commands(cx, None, &[(LONG, "trains wait\n"), (script, "one\ntwo\n")]);

    for ((command, _), (written, case)) in
        opened.into_iter().zip([(LONG, "every row it wraps onto"), (script, "its lines")])
    {
        nothing_copied(&mut ui);
        let shown = ui.bounds(text(&command));
        // From the start of its first line to past the end of its last.
        let (from, to) = (shown.origin + point(px(1.), px(4.)), shown.bottom_right() - point(px(1.), px(4.)));

        ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
        ui.press("secondary-c");

        assert_eq!(copied(&mut ui).as_deref(), Some(written), "{case}, as it was written");
    }
}

#[gpui_kit::test]
fn clicking_a_command_leaves_it_as_it_is_and_a_double_click_selects_a_word(cx: &mut TestAppContext) {
    let (_fake, mut ui, opened) = open_on_commands(cx, None, &[(LONG, "trains wait\n")]);
    let (command, _) = opened[0].clone();
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
    let (_fake, mut ui, opened) = open_on_commands(cx, None, &[(LONG, "trains wait\n")]);
    let (command, output) = opened[0].clone();
    ui.hover(output);
    assert!(!ui.visible(copy(&command)), "hidden while what it printed is hovered");

    ui.hover(text(&command));
    assert!(ui.visible(copy(&command)), "shown while the command is");

    ui.click(copy(&command));

    assert_eq!(copied(&mut ui).as_deref(), Some(LONG));
}

#[gpui_kit::test]
fn a_long_command_scrolls_by_itself_under_its_copy_button(cx: &mut TestAppContext) {
    let script = (1..=30).map(|n| format!("echo signal {n}")).collect::<Vec<_>>().join("\n");
    let printed = signals(30);
    let (_fake, mut ui, opened) = open_on_commands(cx, None, &[(&script, &printed)]);
    let (command, _) = opened[0].clone();
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

/// One row for what's copied once a drag has let go, with a blank line in it, and one for what's selected while the
/// button is still down: each across every line, as it was printed.
#[gpui_kit::test]
fn what_a_command_printed_selects_across_its_lines_as_it_s_dragged_and_copies_as_it_was_printed(
    cx: &mut TestAppContext,
) {
    let commands = [
        (r"printf 'one\n\nthree\nfour\n'", "one\n\nthree\nfour\n"),
        (r"printf 'one\ntwo\nthree\n'", "one\ntwo\nthree\n"),
    ];
    let (_fake, mut ui, opened) = open_on_commands(cx, None, &commands);
    let ((_, blank), (_, three)) = (opened[0].clone(), opened[1].clone());
    assert!(ui.has(ElementId::from((blank.clone(), "lines"))), "drawn a line at a time, rather than as one text");
    let printed = ui.bounds(blank);
    // From the space the block gives it above and before its first line to the space after its last.
    let (from, to) = (printed.origin + point(px(4.), px(4.)), printed.bottom_right() - point(px(4.), px(4.)));

    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("one\n\nthree\nfour"), "let go: with its blank line");

    let (from, to) = across(ui.bounds(three));

    assert_eq!(selected_while_dragged(&mut ui, from, to), "one\ntwo\nthree", "still held: as far as the pointer");
}

/// What's selected of what a command printed stays selected once its row has left the window, and is copied from
/// there.
#[gpui_kit::test]
fn what_a_command_printed_stays_selected_once_its_row_has_left_the_window(cx: &mut TestAppContext) {
    // Enough short paragraphs above it that the transcript can scroll it out of the window, and few enough words that
    // they're parsed as they're drawn.
    let above = (1..=150).map(|n| format!("Entry {n}.")).collect::<Vec<_>>().join("\n\n");
    let (_fake, mut ui, opened) =
        open_on_commands(cx, Some(&above), &[(r"printf 'one\ntwo\nthree\n'", "one\ntwo\nthree\n")]);
    let (_, output) = opened[0].clone();
    let (from, to) = across(ui.bounds(output.clone()));
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();

    for _ in 0..10 {
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(400.))));
    }
    assert!(!ui.visible(output), "its row has left the window");
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("one\ntwo\nthree"));
}

/// A press on what a command printed is on the line under it, as one on the agent's words is: a triple-click selects
/// the line, however it's drawn, and a double-click in the space the block gives it above its first line selects that
/// line's first word, rather than the end of the command above it.
#[gpui_kit::test]
fn a_click_on_what_a_command_printed_selects_from_the_line_under_it(cx: &mut TestAppContext) {
    let many = signals(250);
    let commands = [("seq -f 'signal %g' 1 250", many.as_str()), (r"printf 'printed here\n'", "printed here\n")];
    let (_fake, mut ui, opened) = open_on_commands(cx, None, &commands);
    let ((_, many), (_, here)) = (opened[0].clone(), opened[1].clone());
    // Too much of it to draw a line at a time.
    assert!(!ui.has(ElementId::from((many.clone(), "lines"))), "drawn as one text");
    nothing_copied(&mut ui);

    // On its first line, inside the space around it.
    let thrice = ClickOptions::new().with_offset(inside()).with_count(3);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_with_options(many, thrice, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("signal 1"), "a triple-click: the line, not its first word");

    nothing_copied(&mut ui);
    let twice = ClickOptions::new().with_offset(point(px(14.), px(3.))).with_count(2);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_with_options(here, twice, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("printed"), "a double-click above the first line: its first word");
}

/// A press on the `$` before a command, or in the space around it, is on the command, so a double-click there selects
/// its first word.
#[gpui_kit::test]
fn a_double_click_on_a_command_s_dollar_selects_its_first_word(cx: &mut TestAppContext) {
    let (_fake, mut ui, opened) = open_on_commands(cx, None, &[(LONG, "trains wait\n")]);
    let (command, _) = opened[0].clone();
    let dollar = ui.bounds(text(&command)).left_center() - ui.bounds(command.clone()).origin;

    let twice = ClickOptions::new().with_offset(point(dollar.x - px(10.), px(4.))).with_count(2);
    ui.cx.update_window(ui.handle, |_, window, cx| window.click_with_options(command, twice, cx)).unwrap();
    ui.press("secondary-c");

    assert_eq!(copied(&mut ui).as_deref(), Some("echo"));
}

/// A chat held by a fake hub where the agent said `first`, if anything, then ran each of `commands`, which printed
/// what's beside it, and said it was done. It's open at its end with every command opened, as a click on its line opens
/// it. Gives the IDs of each block's command and of what it printed, from the top.
fn open_on_commands<'a>(
    cx: &'a mut TestAppContext,
    first: Option<&str>,
    commands: &[(&str, &str)],
) -> (FakeHub, Ui<'a>, Vec<(ElementId, ElementId)>) {
    let mut did = Vec::from_iter(first.map(|first| events::text("msg-first", first)));
    for (n, (command, printed)) in commands.iter().enumerate() {
        did.extend(ran(n, command, printed));
    }
    did.push(events::text("msg-done", "done"));
    let (fake, mut ui) = open_on_a_turn(cx, did);
    ui.wait("a line for each command", |ui| ui.labelled(COMMAND_LINE).len() == commands.len());
    // From the bottom up, since a click keeps the line it's on where it was, and what's under it goes down.
    for n in (0..commands.len()).rev() {
        ui.click_labelled(COMMAND_LINE, n);
    }
    to_the_end(&mut ui);
    let mut from_the_top = |part: &str| {
        let mut shown = Vec::from_iter(ui.shown(part).into_iter().map(|id| (ui.bounds(id.clone()).top(), id)));
        shown.sort_by(|(above, _), (below, _)| above.partial_cmp(below).unwrap());
        Vec::from_iter(shown.into_iter().map(|(_, id)| id))
    };
    let (shown, printed) = (from_the_top("command/"), from_the_top("output/"));
    assert_eq!(shown.len(), commands.len(), "every command is open");
    assert_eq!(printed.len(), commands.len(), "with what it printed");
    (fake, ui, shown.into_iter().zip(printed).collect())
}

/// What `seq -f 'signal %g' 1 COUNT` prints: `count` lines, `signal 1` to `signal COUNT`.
fn signals(count: usize) -> String {
    Vec::from_fn(count, |n| format!("signal {}", n + 1)).join("\n") + "\n"
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

/// Empties the clipboard, so what a case copies can't be what one before it left there.
fn nothing_copied(ui: &mut Ui<'_>) {
    ui.cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
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
