//! The card a finished turn's line shows while the pointer rests on it.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, ElementId, Pixels, SharedString, TestAppContext, px};
use protocol::event::EventBody;

use crate::support::{Stack, Ui};

/// A new workspace whose agent has finished one turn on `prompt`, with the pointer resting on the line the turn ended
/// with, which this returns.
fn resting_on_a_turn<'a>(cx: &'a mut TestAppContext, stack: &Stack, prompt: &str) -> (Ui<'a>, SharedString) {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input(prompt);
    ui.press("enter");
    let ended = |ui: &Ui| ui.events().iter().find(|e| matches!(e.body, EventBody::TurnEnded { .. })).map(|e| e.seq);
    ui.wait("the end of the turn in the log", |ui| ended(ui).is_some());
    let line = SharedString::from(format!("turn-end/event/{}/0", ended(&ui).unwrap()));
    ui.wait("its line", |ui| ui.visible(line.clone()));

    ui.hover(line.clone());
    assert!(!ui.has("turn-card"), "passing over the line shows nothing");
    ui.wait_out_a_tooltip();
    (ui, line)
}

#[gpui_kit::test]
fn resting_on_a_finished_turn_shows_what_it_asked_of_the_model_just_above_its_line(cx: &mut TestAppContext) {
    let stack = Stack::new();
    // Long enough to fill the chat, so the line is low enough for the card to fit above it.
    let (mut ui, line) = resting_on_a_turn(cx, &stack, "prose:12:1");

    assert!(ui.visible("turn-card"), "resting on it does");
    // What the fake agent says every turn's own requests take.
    for said in ["Input 6", "Output 550", "Cache read 1.1M", "Cache write 1.6k"] {
        assert_eq!(ui.labelled(said).len(), 1, "{said}");
    }
    // The fake's model takes a minute to start answering, so its request was sent a minute before the hub heard.
    assert_eq!(ui.labelled("Cache 59 min left").len(), 1, "what the hub keeps of the turn's request");
    let (card, line) = (bounds(&mut ui, "turn-card"), bounds(&mut ui, line));
    assert!(card.bottom() <= line.top() && line.top() - card.bottom() < px(8.), "just above: {card:?} {line:?}");
    assert!((card.center().x - line.center().x).abs() < px(1.), "centred on it: {card:?} over {line:?}");
}

#[gpui_kit::test]
fn a_card_with_no_room_above_its_line_moves_down_only_as_far_as_it_has_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, line) = resting_on_a_turn(cx, &stack, "say:hello");

    let (card, line) = (bounds(&mut ui, "turn-card"), bounds(&mut ui, line));
    assert!(line.top() - card.size.height < px(0.), "this line is too near the top for the card: {line:?}");
    assert!(card.top() >= px(0.) && card.top() < px(8.), "in the window, at its top: {card:?}");
    assert!((card.center().x - line.center().x).abs() < px(1.), "still centred on it: {card:?} over {line:?}");
}

fn bounds(ui: &mut Ui<'_>, id: impl Into<ElementId>) -> Bounds<Pixels> {
    let id = id.into();
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap()
}
