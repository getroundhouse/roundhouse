//! Scrolling with the keys, as a page scrolls: Up and Down a line, Page Up and Page Down a screen, and Home and End to
//! the start and the end, in a chat that has focus, a rendered Markdown file and a proposal, Page Up and Page Down from
//! the composer too, which keeps what's typed, and Escape from the chat back to the composer. How far a line and a
//! screen go is tested in `crates/transcript/src/selection`, and here each key in a chat, and one example in each of the
//! others. Only what the hub holds matters, so each is made up for a fake hub.

use client_core::fake_hub::events::text;
use gpui_kit::{AppContext as _, Focusable as _, Pixels, TestAppContext, px};
use look::Region;
use protocol::event::EventBody;
use transcript::Place;

use super::selecting::{PARAGRAPHS, last_shown, on_the_first, painted, paragraphs, with_a_file, with_a_proposal};
use super::{A_FRAME, open_on_a_chat, prose, thumb, turns};
use crate::support::Ui;

/// A chat of a few turns, each with a reply a screen or two long, so it's several screens long, which a fake hub loads
/// at once.
fn a_chat_screens_long() -> Vec<EventBody> {
    turns(Vec::from_fn(4, |n| (format!("Prompt {n}."), vec![text(&format!("msg-{n}"), &prose(12, n))])))
}

/// How far down the chat the top of what it shows is.
fn scrolled(ui: &Ui<'_>) -> Pixels {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().scrolled())
}

/// Presses `key`, and says how far down that scrolled the chat, or up for less than nothing.
fn scrolled_by(ui: &mut Ui<'_>, key: &str) -> Pixels {
    let was = scrolled(ui);
    ui.press(key);
    scrolled(ui) - was
}

/// Gives the chat focus, as a press in it does.
fn focus_the_chat(ui: &mut Ui<'_>) {
    let chat = ui.bounds(Region::Transcript.id());
    ui.click_at(chat.center());
}

/// Whether the composer has focus.
fn composer_focused(ui: &mut Ui<'_>) -> bool {
    let pane = ui.pane().unwrap();
    ui.cx.update_window(ui.handle, |_, window, cx| pane.read(cx).composer.focus_handle(cx).is_focused(window)).unwrap()
}

/// In a chat that has focus, Up and Down scroll it a line, Page Up and Page Down a screen, Home goes to its first row,
/// and End back to its end.
#[gpui_kit::test]
fn the_keys_scroll_a_chat_that_has_focus_as_a_page(cx: &mut TestAppContext) {
    let (_fake, mut ui, _) = open_on_a_chat(cx, a_chat_screens_long());
    let tall = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().viewport().size.height);
    let line = ui.cx.update_window(ui.handle, |_, window, _| window.line_height()).unwrap();
    focus_the_chat(&mut ui);

    let a_screen_up = scrolled_by(&mut ui, "pageup");
    let a_line_up = scrolled_by(&mut ui, "up");
    let a_line_down = scrolled_by(&mut ui, "down");
    let a_screen_down = scrolled_by(&mut ui, "pagedown");
    ui.press("home");
    let start = ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| t.list().top());
    ui.press("end");
    ui.settle(A_FRAME);

    let near = |by: Pixels, should: Pixels| (by - should).abs() < px(1.);
    assert!(near(a_screen_up, -tall * 0.875), "a screen up, less an eighth of it, not {a_screen_up:?} of {tall:?}");
    assert!(near(a_line_up, -line), "a line up, not {a_line_up:?}");
    assert!(near(a_line_down, line), "a line down, not {a_line_down:?}");
    assert!(near(a_screen_down, tall * 0.875), "a screen down, not {a_screen_down:?}");
    assert_eq!(start, Place { row: 0, offset: px(0.) }, "Home goes to the first row");
    assert!((thumb(&ui) - 1.).abs() < f32::EPSILON, "and End to the end, not {}", thumb(&ui));
}

/// Page Up and Page Down scroll the chat from the composer, and leave what's typed there as it was, with the caret
/// where it was, where the composer would have moved it a screen.
#[gpui_kit::test]
fn page_up_and_page_down_scroll_the_chat_from_the_composer(cx: &mut TestAppContext) {
    let (_fake, mut ui, _) = open_on_a_chat(cx, a_chat_screens_long());
    ui.input("Signals clear\nahead of the train\nat the box");
    let pane = ui.pane().unwrap();
    let caret = |ui: &Ui<'_>| ui.cx.read_entity(&pane, |p, cx| p.composer.read(cx).cursor());
    let (typed, at) = (ui.composer_text(), caret(&ui));
    let end = scrolled(&ui);

    ui.press("pageup");
    let up = scrolled(&ui);
    ui.press("pagedown");

    assert!(up < end, "Page Up scrolled the chat up, to {up:?} from {end:?}");
    assert_eq!(scrolled(&ui), end, "and Page Down back down");
    assert_eq!((ui.composer_text(), caret(&ui)), (typed, at), "leaving what's typed as it was");
    assert!(composer_focused(&mut ui), "with the composer still focused");
}

/// Escape in a chat that has focus closes Find's bar first, if it's open over the chat, and otherwise goes back to the
/// composer. Page Up scrolls the chat from Find's bar too.
#[gpui_kit::test]
fn escape_goes_from_the_chat_back_to_the_composer_once_find_s_bar_has_closed(cx: &mut TestAppContext) {
    let (_fake, mut ui, _) = open_on_a_chat(cx, a_chat_screens_long());
    let transcript = ui.transcript().unwrap();
    let finding = |ui: &Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.is_finding());
    ui.press("secondary-f");
    ui.wait("Find's bar", |ui| finding(ui));
    let from_the_bar = scrolled_by(&mut ui, "pageup");
    focus_the_chat(&mut ui);

    ui.press("escape");
    let closed = !finding(&ui);
    focus_the_chat(&mut ui);
    let in_the_chat = !composer_focused(&mut ui);
    ui.press("escape");

    assert!(from_the_bar < px(0.), "Page Up scrolls the chat from Find's bar");
    assert!(closed, "the first Escape closes Find's bar");
    assert!(in_the_chat, "and a press gives the chat focus again");
    assert!(composer_focused(&mut ui), "and the next goes back to the composer");
}

/// A rendered Markdown file that has focus goes to its end with End and back to its start with Home, and down a
/// screen with Page Down.
#[gpui_kit::test]
fn the_keys_scroll_a_rendered_markdown_file(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_file(cx, &paragraphs("Paragraph"));
    ui.click("all-file-long.md");
    ui.wait("the file, rendered", |ui| painted(ui, "Paragraph 1.").is_some());
    let first = on_the_first(&mut ui, "Paragraph");
    ui.click_at(first);
    let shown = last_shown(&mut ui, "Paragraph");

    ui.press("pagedown");
    let a_screen_on = painted(&mut ui, &format!("Paragraph {}.", shown + 1)).is_some();
    ui.press("end");
    let at_the_end = painted(&mut ui, &format!("Paragraph {PARAGRAPHS}.")).is_some();
    ui.press("home");

    assert!(a_screen_on, "Page Down shows the paragraph after the last that showed");
    assert!(at_the_end, "End shows the last");
    assert!(painted(&mut ui, "Paragraph 1.").is_some(), "and Home the first again");
}

/// A proposal that has focus goes down a screen with Page Down, and to its end with End.
#[gpui_kit::test]
fn the_keys_scroll_a_proposal(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_proposal(cx, &paragraphs("Context"));
    ui.wait("the context", |ui| painted(ui, "Context 1.").is_some());
    let first = on_the_first(&mut ui, "Context");
    ui.click_at(first);
    let shown = last_shown(&mut ui, "Context");

    ui.press("pagedown");
    let a_screen_on = painted(&mut ui, &format!("Context {}.", shown + 1)).is_some();
    ui.press("end");

    assert!(a_screen_on, "Page Down shows the paragraph after the last that showed");
    assert!(painted(&mut ui, &format!("Context {PARAGRAPHS}.")).is_some(), "and End the last");
}
