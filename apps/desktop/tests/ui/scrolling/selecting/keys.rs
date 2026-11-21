//! Selecting with the keys: Shift with an arrow moves whichever end of what's selected moved last, as in any text, a
//! character, a word or a line at a time, through plain text and Markdown alike, and the chat, and a block that scrolls
//! what a command printed, scroll to keep it in view, as they do for a drag held past their edge. How each key moves
//! the end, from one text into the next and from line to line, is tested in `crates/transcript/src/selection/keys`, and
//! here only that each key does, and that the chat and its blocks scroll to the end. Only what the hub recorded
//! matters, so each chat is made up for a fake hub.

use gpui_kit::{AppContext as _, ElementId, TestAppContext, point, px};

use super::super::{A_FRAME, turns};
use super::kinds::settled_at_its_last_row;
use super::made_up::{command, open_on_a_chat, open_on_chats_of, reply};
use super::{COMMAND_LINE, bounds, click, on_the_reply_saying};
use crate::support::Ui;

/// What's selected now, as a copy would have it, which is nothing with nothing selected.
fn selected(ui: &Ui<'_>) -> Option<String> {
    let transcript = ui.transcript().expect("the chat");
    let selected = ui.cx.read_entity(&transcript, |t, _| t.as_copied());
    (!selected.is_empty()).then_some(selected)
}

/// Whether the end of what's selected that moved last is in sight, if it's drawn.
fn in_sight(ui: &Ui<'_>) -> Option<bool> {
    let transcript = ui.transcript().expect("the chat");
    ui.cx.read_entity(&transcript, |t, _| t.selected_end_in_sight())
}

/// Presses `keys`, `times` times, with the frames each asks for played after it, as a display brings them between
/// the presses a key held down repeats.
fn keys(ui: &mut Ui<'_>, keys: &str, times: usize) {
    for _ in 0..times {
        ui.press(keys);
        ui.settle(A_FRAME);
    }
}

/// The keys that move an end a word right or left, `way`: with Option on a Mac, and Ctrl elsewhere.
fn by_word(ui: &mut Ui<'_>, way: &str) -> String {
    let held = if ui.cx.update(|cx| cx.platform_info().is_mac()) { "alt-shift" } else { "ctrl-shift" };
    format!("{held}-{way}")
}

/// The chat scrolled to its start, or to its end with `latest`.
fn go(ui: &mut Ui<'_>, latest: bool) {
    let transcript = ui.transcript().expect("the chat");
    ui.cx.update_entity(&transcript, |t, cx| {
        if latest {
            t.list().go_to_latest();
        } else {
            t.list().go_to_row(0);
        }
        cx.notify();
    });
    ui.settle(A_FRAME);
}

/// Left and right go a character or a word at a time through a command, and down goes a line at a time through what it
/// printed, into the lines its block has scrolled out of sight, which the block scrolls to.
#[gpui_kit::test]
fn the_keys_move_the_end_through_a_command_and_the_lines_its_block_scrolls_to(cx: &mut TestAppContext) {
    let printed = Vec::from_fn(21, |n| (100 + n).to_string()).join("\n");
    let did = command(1, "echo trains wait at the signal", &printed);
    let (_fake, mut ui, ..) = open_on_chats_of(cx, vec![turns(vec![("Run it.".to_owned(), did)])]);
    ui.click_labelled(COMMAND_LINE, 0);
    settled_at_its_last_row(&mut ui);
    let (command, output) = (ui.shown("command/").remove(0), ui.shown("output/").remove(0));
    let command = bounds(&mut ui, ElementId::from((command, "text")));
    let output = bounds(&mut ui, output);

    click(&mut ui, command.origin + point(px(4.), px(6.)), 2);
    let word = selected(&ui);
    keys(&mut ui, "shift-right", 3);
    let characters = selected(&ui);
    let word_right = by_word(&mut ui, "right");
    keys(&mut ui, &word_right, 1);
    let to_its_end = selected(&ui);
    let word_left = by_word(&mut ui, "left");
    keys(&mut ui, &word_left, 2);
    let back = selected(&ui);
    // On its first line, inside the space the block gives it.
    click(&mut ui, output.origin + point(px(18.), px(16.)), 2);
    keys(&mut ui, "shift-down", 18);

    assert_eq!(word.as_deref(), Some("echo"));
    assert_eq!(characters.as_deref(), Some("echo tr"), "a character at a time");
    assert_eq!(to_its_end.as_deref(), Some("echo trains"), "and to the end of the word");
    assert_eq!(back, None, "and back to where it started, with nothing selected");
    let lines = Vec::from_fn(19, |n| (100 + n).to_string()).join("\n");
    assert_eq!(selected(&ui), Some(lines), "a line at a time through what it printed");
    assert_eq!(in_sight(&ui), Some(true), "in view in its block");
}

/// Down goes through the rows of a reply's Markdown, which the kit lays out, and on below the window into the replies
/// after it, with the chat scrolling to keep the end in view at each press, by a line once it's at the window's bottom,
/// as it does under a drag held just past it, and on past the end of the turn. Up comes back the same way, and on past
/// the window's top.
#[gpui_kit::test]
fn a_run_of_shift_down_goes_on_past_the_window_s_bottom_a_line_at_a_time_and_up_comes_back(cx: &mut TestAppContext) {
    let long = "Trains wait at a stop signal until it clears and the box says the line ahead is free. ".repeat(30);
    let long = long.trim_end().to_owned();
    // The short replies are in a turn of their own, so on the way to them is the end of the first turn, a row with no
    // text that's taller than a line, which the chat scrolls past a line a frame.
    let short = Vec::from_fn(4, |n| reply(&format!("Reply {n} is short.")));
    let turns = vec![("Keep the ledger.".to_owned(), vec![reply(&long)]), ("And the rest.".to_owned(), short)];
    let (_fake, mut ui, window) = open_on_a_chat(cx, turns);
    go(&mut ui, false);
    let at = on_the_reply_saying(&ui, window, &long);
    click(&mut ui, at, 2);
    let word = selected(&ui);
    let transcript = ui.transcript().unwrap();
    let top = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.list().top());

    // Until the end is in the last reply, partway along it, across from where it started.
    let mut down = Vec::new();
    while selected(&ui).is_none_or(|selected| !selected.contains("Reply 2 is short.\n\n")) && down.len() < 60 {
        keys(&mut ui, "shift-down", 1);
        // The reply is one paragraph, so what's selected has a blank line in it once it goes on into another text, but
        // for the first place in that text, which a copy has nothing of, so it has all of the reply to its end.
        let in_the_reply = selected(&ui).filter(|selected| !selected.contains("\n\n") && !selected.ends_with("free."));
        down.push((top(&ui), in_sight(&ui), in_the_reply));
    }
    let presses = down.len();
    let up = Vec::from_fn(presses, |_| {
        keys(&mut ui, "shift-up", 1);
        in_sight(&ui)
    });
    let back = selected(&ui);
    // And one more, into the prompt above the reply, which the chat has scrolled out of sight above the window, so it
    // scrolls up a line a frame to it.
    keys(&mut ui, "shift-up", 1);

    assert!(word.as_deref().is_some_and(|word| !word.contains(' ')), "a word of it: {word:?}");
    assert!(presses < 60, "it went on into the replies below: {:?}", selected(&ui));
    // How far the chat moved at each press that moved the end a line within the reply, while the same row was at the
    // top, which from the first that moved it, once the end reached the window's bottom, is as far as the end moved: a
    // line, or the part of one that was past the bottom. A line of the reply is under 40px, and a window of it many
    // times that. A press past the reply's last line scrolls a line a frame until the next text shows, and that's more.
    let moves = Vec::from_iter(
        down.windows(2)
            .take_while(|two| two[1].2.is_some() && two[1].2 != two[0].2)
            .filter(|two| two[0].0.row == two[1].0.row)
            .map(|two| two[1].0.offset - two[0].0.offset),
    );
    let from_the_bottom = moves.iter().position(|by| *by > px(0.)).map_or(&[][..], |at| &moves[at..]);
    let a_line_at_most = from_the_bottom.iter().all(|by| *by > px(0.) && *by < px(40.));
    assert!(from_the_bottom.len() > 3 && a_line_at_most, "a line at each press from the bottom on: {moves:?}");
    let down_in_sight = Vec::from_iter(down.iter().map(|(_, in_sight, _)| *in_sight));
    assert!(
        down_in_sight.iter().chain(&up).all(|it| *it == Some(true)),
        "in view after each: {down_in_sight:?} {up:?}"
    );
    assert_eq!(back, word, "and back up to where it started");
    let above = selected(&ui).unwrap_or_default();
    assert!(above.starts_with("Keep the ledger."), "and on into the prompt above: {above:?}");
}

/// A key brings the end it moves back into view after the chat's been scrolled away from it, as in any text. This is
/// the one thing the keys do that a drag never needs, since a drag's end is under the pointer.
#[gpui_kit::test]
fn a_key_brings_the_end_back_into_view_after_the_chat_has_been_scrolled_away(cx: &mut TestAppContext) {
    let first = "Signal boxes keep ledgers of every train.";
    let mut said = vec![reply(first)];
    said.extend(Vec::from_fn(40, |n| reply(&format!("Reply {n} is short."))));
    let (_fake, mut ui, window) = open_on_a_chat(cx, vec![("Keep the ledger.".to_owned(), said)]);
    go(&mut ui, false);
    let at = on_the_reply_saying(&ui, window, first);
    click(&mut ui, at, 2);
    let word = selected(&ui).unwrap_or_default();
    go(&mut ui, true);
    let away = in_sight(&ui);

    keys(&mut ui, "shift-right", 1);

    assert_eq!(away, None, "scrolled away, it isn't drawn");
    let moved = selected(&ui).unwrap_or_default();
    assert!(moved.len() == word.len() + 1 && moved.starts_with(&word), "a character on: {word:?} to {moved:?}");
    assert_eq!(in_sight(&ui), Some(true), "and it's back in view");
}

/// At the chat's start and at its end, a key with nowhere further to go stops, and asks for no more frames, as
/// [`Ui::settle`] checks.
#[gpui_kit::test]
fn at_the_chat_s_start_and_end_a_key_with_nowhere_to_go_stops(cx: &mut TestAppContext) {
    let (first, last) = ("Signal boxes keep ledgers.", "Trains wait at the signal.");
    let (_fake, mut ui, window) =
        open_on_a_chat(cx, vec![("Keep the ledger.".to_owned(), vec![reply(first), reply(last)])]);
    let at = on_the_reply_saying(&ui, window, first);
    click(&mut ui, at, 1);
    keys(&mut ui, "shift-up", 4);
    keys(&mut ui, "shift-left", 30);
    let to_the_start = selected(&ui).unwrap_or_default();
    let at = on_the_reply_saying(&ui, window, last);
    click(&mut ui, at, 1);
    keys(&mut ui, "shift-down", 4);
    keys(&mut ui, "shift-right", 30);
    let to_the_end = selected(&ui).unwrap_or_default();

    assert!(to_the_start.starts_with("Keep the ledger."), "back to the chat's start: {to_the_start:?}");
    assert!(to_the_end.ends_with("signal."), "and on to its end: {to_the_end:?}");
}

/// With Find's bar open over the top of the chat, a run of Shift-Up on past the window's top keeps the end below the
/// bar, as Find keeps the match it shows.
#[gpui_kit::test]
fn with_find_open_a_run_of_shift_up_keeps_the_end_below_its_bar(cx: &mut TestAppContext) {
    let said = Vec::from_fn(40, |n| reply(&format!("Reply {n} is short.")));
    let (_fake, mut ui, window) = open_on_a_chat(cx, vec![("Keep the ledger.".to_owned(), said)]);
    go(&mut ui, true);
    ui.press("secondary-f");
    ui.settle(A_FRAME);
    let at = on_the_reply_saying(&ui, window, "Reply 39 is short.");
    click(&mut ui, at, 2);

    let up = Vec::from_fn(30, |_| {
        keys(&mut ui, "shift-up", 1);
        in_sight(&ui)
    });

    assert!(ui.has("find-bar"), "with the bar still open");
    // It keeps to where it was across, so it's partway along the reply it's reached.
    let selected = selected(&ui).unwrap_or_default();
    let first = selected.lines().next().unwrap_or_default();
    assert!(first.ends_with(" 9 is short."), "on past the window's top: {first:?}");
    assert!(up.iter().all(|it| *it == Some(true)), "below the bar after each: {up:?}");
}
