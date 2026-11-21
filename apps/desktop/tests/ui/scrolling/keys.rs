//! Scrolling with the keys, as a page scrolls: Up and Down a line, Page Up and Page Down a screen, and Home and End to
//! the start and the end, in a chat that has focus, a rendered Markdown file and a proposal, Page Up and Page Down from
//! the composer too, which keeps what's typed, and Escape from the chat back to the composer. `Option-Up` and
//! `Option-Down` step a Markdown file between its headings and a proposal between its parts here, and a chat between its
//! prompts in `outline::steps`. How far a line and a screen go is tested in `crates/transcript/src/selection`, and here
//! each key in a chat, and one example in each of the others. Only what the hub holds matters, so each is made up for a
//! fake hub.

use client_core::fake_hub::events::text;
use gpui_kit::base::test_support::texts;
use gpui_kit::{AppContext as _, Focusable as _, Pixels, TestAppContext, point, px};
use look::Region;
use protocol::event::EventBody;
use transcript::Place;

use super::selecting::{
    PARAGRAPHS, last_shown, on_the_first, painted, paragraphs, with_a_file, with_a_proposal, with_a_proposal_asking,
};
use super::selecting::{click, copy};
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

/// The document's texts that show, from the top, of those that say `either` or `or` at their start.
fn shown_from_the_top(ui: &mut Ui<'_>, either: &str, or: &str) -> Vec<String> {
    ui.frame();
    let shown = |window: &mut gpui_kit::Window| {
        let mut shown: Vec<_> = texts(window)
            .into_iter()
            .filter(|text| text.visible() && (text.text().starts_with(either) || text.text().starts_with(or)))
            .map(|text| (text.bounds().top(), text.text().to_owned()))
            .collect();
        shown.sort_by(|a, b| f32::from(a.0).total_cmp(&f32::from(b.0)));
        shown.into_iter().map(|(_, text)| text).collect()
    };
    ui.cx.update_window(ui.handle, |_, window, _| shown(window)).unwrap()
}

/// A Markdown file of four sections, each a heading and 30 paragraphs, far longer than a screen.
fn sections() -> String {
    let sections = Vec::from_fn(4, |s| {
        let paragraphs = Vec::from_fn(30, |n| format!("Paragraph {}.{}.", s + 1, n + 1)).join("\n\n");
        format!("# Section {}\n\n{paragraphs}", s + 1)
    });
    sections.join("\n\n")
}

/// `Option-Down` steps a rendered Markdown file on to the heading after the top, with that at the top, and
/// `Option-Up` back, to the top of the heading's part the file is scrolled into first, and then to the one before.
#[gpui_kit::test]
fn option_up_and_down_step_a_rendered_markdown_file_between_its_headings(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_file(cx, &sections());
    ui.click("all-file-long.md");
    ui.wait("the file, rendered", |ui| painted(ui, "Section 1").is_some());
    let first = painted(&mut ui, "Paragraph 1.1.").unwrap();
    ui.click_at(first.origin + point(px(2.), px(4.)));
    let at_the_top = |ui: &mut Ui<'_>| shown_from_the_top(ui, "Section", "Paragraph").into_iter().next();

    ui.press("alt-down");
    let on = at_the_top(&mut ui);
    ui.press("alt-down");
    let on_again = at_the_top(&mut ui);
    ui.press("down");
    ui.press("alt-up");
    let back_to_its_top = at_the_top(&mut ui);
    ui.press("alt-up");

    assert_eq!(on.as_deref(), Some("Section 2"), "the next heading, at the top");
    assert_eq!(on_again.as_deref(), Some("Section 3"));
    assert_eq!(back_to_its_top.as_deref(), Some("Section 3"), "back to the top of the heading scrolled into");
    assert_eq!(at_the_top(&mut ui).as_deref(), Some("Section 2"), "and then to the one before");
}

/// `Option-Down` steps a proposal on from its context to its question, and `Option-Up` back.
#[gpui_kit::test]
fn option_up_and_down_step_a_proposal_between_its_parts(cx: &mut TestAppContext) {
    // The question's details, which are Markdown, unlike what it asks, are far longer than a screen.
    let context = "Context 1.\n\nContext 2.";
    let (_fake, mut ui) = with_a_proposal_asking(cx, context, &paragraphs("Detail"));
    ui.wait("the context", |ui| painted(ui, "Context 1.").is_some());
    let first = on_the_first(&mut ui, "Context");
    ui.click_at(first);

    ui.press("alt-down");
    let on = (painted(&mut ui, "Context 1.").is_some(), painted(&mut ui, "Detail 1.").is_some());
    ui.press("alt-up");

    assert_eq!(on, (false, true), "the question, at the top, past the context");
    assert!(painted(&mut ui, "Context 1.").is_some(), "and back to the context");
}

/// With Shift, `Option-Down` moves the end of what's selected in a rendered Markdown file to the start of the heading
/// after it, and `Option-Up` back to the start of the one before.
#[gpui_kit::test]
fn option_shift_up_and_down_move_the_end_to_the_start_of_a_heading(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_file(cx, &sections());
    ui.click("all-file-long.md");
    ui.wait("the file, rendered", |ui| painted(ui, "Section 1").is_some());
    let word = painted(&mut ui, "Paragraph 1.3.").unwrap();
    click(&mut ui, word.origin + point(px(2.), px(4.)), 2);

    ui.press("alt-shift-down");
    let on = copy(&mut ui);
    ui.press("alt-shift-up");
    let back = copy(&mut ui);

    assert!(on.starts_with("Paragraph 1.3.") && on.trim_end().ends_with("Paragraph 1.30."), "to Section 2: {on:?}");
    assert!(back.starts_with("Section 1\n\nParagraph 1.1."), "and back to Section 1: {back:?}");
}

/// With Shift, `Option-Up` moves the end of what's selected in a proposal to the start of the part it's in, and then
/// to the start of the one before.
#[gpui_kit::test]
fn option_shift_up_moves_the_end_to_the_start_of_a_proposal_s_part(cx: &mut TestAppContext) {
    let (_fake, mut ui) = with_a_proposal_asking(cx, "Context 1.\n\nContext 2.", &paragraphs("Detail"));
    ui.wait("the context", |ui| painted(ui, "Context 1.").is_some());
    let word = painted(&mut ui, "Detail 3.").unwrap();
    click(&mut ui, word.origin + point(px(2.), px(4.)), 2);

    ui.press("alt-shift-up");
    let to_its_part = copy(&mut ui);
    ui.press("alt-shift-up");

    assert!(to_its_part.starts_with("Where does a held train's wait live?"), "its question: {to_its_part:?}");
    let to_the_one_before = copy(&mut ui);
    assert!(to_the_one_before.starts_with("Context 1."), "then the context: {to_the_one_before:?}");
}
