//! Retry, on the card of a request to the model that failed, which asks the agent to carry on.

#![cfg(test)]

use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use protocol::event::EventBody;
use protocol::model::Seq;

use crate::support::{Stack, Ui, click};

fn retry(card: Seq) -> ElementId {
    ElementId::NamedInteger("retry".into(), card)
}

fn prompts(ui: &Ui) -> Vec<String> {
    let said = |body: &EventBody| match body {
        EventBody::UserPrompt { prompt, .. } => Some(prompt.text.clone()),
        _ => None,
    };
    ui.events().iter().filter_map(|e| said(&e.body)).collect()
}

#[gpui_kit::test]
fn retry_asks_the_agent_to_carry_on_once_and_goes_as_soon_as_its_pressed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input("fail:rate_limit");
    ui.press("enter");
    let failed =
        |ui: &Ui| ui.events().iter().find(|e| matches!(e.body, EventBody::RequestFailed { .. })).map(|e| e.seq);
    ui.wait("the failed request", |ui| failed(ui).is_some());
    let card = failed(&ui).unwrap();
    ui.wait("its Retry", |ui| ui.visible(retry(card)));

    let transcript = ui.transcript().unwrap();
    assert_eq!(ui.cx.read_entity(&transcript, |t, _| t.retry_offered_at()), Some(card));

    // Read as the click is handled, before anything from the hub can arrive, so it's the prompt on its way that does it.
    let offered = ui
        .cx
        .update_window(ui.handle, |_, window, cx| {
            click(window, retry(card), cx);
            transcript.read(cx).retry_offered_at()
        })
        .unwrap();

    assert_eq!(offered, None, "gone as soon as it's pressed, so it can't be pressed twice");
    let ended = |ui: &Ui| ui.events().iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    ui.wait("the turn it started to end", |ui| ended(ui) == 2);
    assert_eq!(prompts(&ui), ["fail:rate_limit", "Continue"]);
    assert!(!ui.has(retry(card)), "the agent carried on past it");
}
