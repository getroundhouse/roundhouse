//! Stepping between a chat's prompts with `Option-Up` and `Option-Down`, which put each where a click on it in the
//! outline's list does, from the composer, leaving what's typed, and from the chat. Where a step goes from where the
//! chat is, a compaction and an answer to a proposal too, is tested in `crates/transcript/src/transcript/outline`.

use gpui_kit::{AppContext as _, TestAppContext};
use look::Region;

use super::jumps::assert_at_the_bottom;
use super::worlds::{Turns, open_on_a_chat};
use super::{prompt_rows, row_id, scrolled_to, selected};
use crate::finding::{FRAME, find, settle_with_the_match_in_view};
use crate::support::Ui;

/// Presses `key`, and plays the frames it takes to lay the rows out where it sent the chat.
fn step(ui: &mut Ui<'_>, key: &str) {
    ui.press(key);
    ui.bring_the_next_frame();
    ui.frame();
}

/// Whether the chat is at its end.
fn at_the_end(ui: &Ui<'_>) -> bool {
    ui.cx.read_entity(&ui.transcript().unwrap(), |t, _| (t.list().thumb().0 - 1.).abs() < f32::EPSILON)
}

/// From the composer, `Option-Up` steps back from the end to the last prompt, then to the one before, and
/// `Option-Down` on again, and from the last prompt to the end, leaving what's typed. From the chat, they step it too.
#[gpui_kit::test]
fn option_up_and_down_step_the_chat_between_its_prompts(cx: &mut TestAppContext) {
    // Long replies, so every prompt but the first has more than a screen of chat above it.
    let turns = (0..6).fold(Turns::default(), |turns, turn| turns.prose(4, turn));
    let (_fake, mut ui, _) = open_on_a_chat(cx, turns);
    ui.wait("the outline", |ui| ui.has("outline"));
    let rows = prompt_rows(&ui);
    ui.wait("the end of the chat", |ui| scrolled_to(ui) > rows[3]);
    ui.input("Signals clear");

    step(&mut ui, "alt-up");
    assert_at_the_bottom(&mut ui, rows[5], "the last prompt, from the end");
    step(&mut ui, "alt-up");
    assert_at_the_bottom(&mut ui, rows[4], "the one before");
    assert!(selected(&mut ui, row_id("outline-tick", rows[4])), "and it's the one being read");
    step(&mut ui, "alt-down");
    assert_at_the_bottom(&mut ui, rows[5], "the one after");
    step(&mut ui, "alt-down");
    assert!(at_the_end(&ui), "and from the last, the end");
    assert_eq!(ui.composer_text(), "Signals clear", "leaving what's typed");

    let chat = ui.bounds(Region::Transcript.id());
    ui.click_at(chat.center());
    step(&mut ui, "alt-up");
    assert_at_the_bottom(&mut ui, rows[5], "the last prompt, from the chat");
}

/// A step pressed in the frame after Find goes on to a match far down the chat, while the chat is on its way there,
/// goes where it sends the chat. The match gives way, as it does to the keys that scroll a page, rather than go on
/// being brought into view after the step.
#[gpui_kit::test]
fn a_step_as_find_goes_to_a_match_goes_where_it_sends_the_chat(cx: &mut TestAppContext) {
    let between = Vec::from_fn(60, |part| format!("Part {part}.")).join("\n\n");
    let turns = Turns::default().prose(4, 0).turn(&[&format!("A needle here.\n\n{between}\n\nA needle there.")]);
    let (_fake, mut ui, _) = open_on_a_chat(cx, turns);
    ui.wait("the outline", |ui| ui.has("outline"));
    let rows = prompt_rows(&ui);
    find(&mut ui, "needle");
    settle_with_the_match_in_view(&mut ui);

    ui.press_a_frame_apart(&["enter", "alt-up"]);
    ui.settle(FRAME);

    assert_at_the_bottom(&mut ui, rows[1], "the prompt the step went back to, rather than the match");
}
