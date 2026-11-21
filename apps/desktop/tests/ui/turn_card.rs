//! The card a finished turn's line shows while the pointer rests on it. What it says is what the hub holds of the turn,
//! and where it goes is the app's own, so these open on a fake hub. The backend's `prompt_cache` tests have what the
//! hub keeps of a turn's request.

#![cfg(test)]

use client_core::fake_hub::events::{finished_turn, first_turn, text, user_prompt};
use client_core::fake_hub::{NOW_MS, World};
use gpui_kit::{SharedString, TestAppContext, px};
use protocol::model::SessionStatus;

use crate::support::{FakeHub, Ui, open_on_the_world};

/// A workspace whose agent has finished one turn, saying each of `said`, with its cache used a minute before now and
/// the time held there, with the pointer resting on the line the turn ended with, which this returns. The hub goes with
/// the test, so it's returned to be kept.
fn resting_on_a_turn<'a>(cx: &'a mut TestAppContext, said: &[&str]) -> (FakeHub, Ui<'a>, SharedString) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let mut events = first_turn(user_prompt("Say something"), []);
    events.extend(said.iter().enumerate().map(|(i, said)| text(&format!("msg-{i}"), said)));
    events.push(finished_turn(4, 0.01));
    let line = SharedString::from(format!("turn-end/event/{}/0", events.len()));
    let chat = world.chat(&workspace, SessionStatus::Idle, events);
    world.cached(&chat, 1);
    let (fake, mut ui) = open_on_the_world(cx, world);
    ui.cx.update(|cx| look::hold_still(NOW_MS, cx));
    ui.wait("its line", |ui| ui.visible(line.clone()));

    ui.hover(line.clone());
    assert!(!ui.has("turn-card"), "passing over the line shows nothing");
    ui.wait_out_a_tooltip();
    (fake, ui, line)
}

#[gpui_kit::test]
fn resting_on_a_finished_turn_shows_what_it_asked_of_the_model_just_above_its_line(cx: &mut TestAppContext) {
    // Enough lines to put the turn's line low enough for the card to fit above it, each a short one, so there's
    // little text to lay out.
    let lines = Vec::from_fn(16, |i| format!("Line {i} of the answer."));
    let (_fake, mut ui, line) = resting_on_a_turn(cx, &Vec::from_iter(lines.iter().map(String::as_str)));

    assert!(ui.visible("turn-card"), "resting on it does");
    // What the fake hub's turns ask of the model.
    for said in ["Input 6", "Output 1.8k", "Cache read 412.4k", "Cache write 9.6k"] {
        assert_eq!(ui.labelled(said).len(), 1, "{said}");
    }
    assert_eq!(ui.labelled("Cache 59 min left").len(), 1, "what the hub keeps of the turn's request");
    let (card, line) = (ui.bounds("turn-card"), ui.bounds(line));
    assert!(card.bottom() <= line.top() && line.top() - card.bottom() < px(8.), "just above: {card:?} {line:?}");
    assert!((card.center().x - line.center().x).abs() < px(1.), "centred on it: {card:?} over {line:?}");
}

#[gpui_kit::test]
fn a_card_with_no_room_above_its_line_moves_down_only_as_far_as_it_has_to(cx: &mut TestAppContext) {
    let (_fake, mut ui, line) = resting_on_a_turn(cx, &["hello"]);

    let (card, line) = (ui.bounds("turn-card"), ui.bounds(line));
    assert!(line.top() - card.size.height < px(0.), "this line is too near the top for the card: {line:?}");
    assert!(card.top() >= px(0.) && card.top() < px(8.), "in the window, at its top: {card:?}");
    assert!((card.center().x - line.center().x).abs() < px(1.), "still centred on it: {card:?} over {line:?}");
}
