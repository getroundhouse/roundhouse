//! Retry, on the card of a request to the model that failed, which asks the agent to carry on, and failures in a row
//! as one card.

#![cfg(test)]

use gpui_kit::{AppContext as _, ElementId, ScrollDelta, TestAppContext, point, px};
use protocol::event::EventBody;
use protocol::model::Seq;

use crate::support::{Gate, Stack, Ui, click, under_the_middle_of};

fn retry(card: Seq) -> ElementId {
    ElementId::NamedInteger("retry".into(), card.get())
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
    let (offered, under) = ui
        .cx
        .update_window(ui.handle, |_, window, cx| {
            click(window, retry(card), cx);
            let offered = transcript.read(cx).retry_offered_at();
            // Only when it's still there, as a click that did nothing leaves it.
            (offered, offered.map(|_| under_the_middle_of(window, retry(card))))
        })
        .unwrap();

    assert_eq!(offered, None, "gone as soon as it's pressed, so it can't be pressed twice. Under the click: {under:?}");
    let ended = |ui: &Ui| ui.events().iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    ui.wait("the turn it started to end", |ui| ended(ui) == 2);
    assert_eq!(prompts(&ui), ["fail:rate_limit", "Continue"]);
    assert!(!ui.has(retry(card)), "the agent carried on past it");
}

/// The agent's own failures in a row, as the hub says of each, are one card: refused partway through a turn, then as
/// it picks up by itself after each of its two tasks in the background, the newest card says all three. What it says
/// is measured again as each comes, so it takes no room where it was drawn: the cards before it, and the line of each
/// turn the agent started by itself, drawn while its request waited, even with the chat scrolled up away from them,
/// where only what's out of date is drawn again.
#[gpui_kit::test]
fn failures_in_a_row_are_one_card_as_each_arrives(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let file = |name: &str| stack.dir.path().join(name);
    let (first, second) = (file("first").display().to_string(), file("second").display().to_string());
    // Long enough that scrolled up, the cards and lines are out of sight, and no longer, since a reply streamed a
    // paragraph at a time is laid out again as each comes.
    ui.paste_text(&format!("prose:20:1; background:{first}; background:{second}; fail:rate_limit"));
    ui.press("enter");
    let failures = |ui: &Ui| ui.events().iter().filter(|e| matches!(e.body, EventBody::RequestFailed { .. })).count();
    ui.wait("the first failure's card", |ui| failures(ui) == 1 && ui.shown("failed-request").len() == 1);
    assert!(ui.shown("failed-try").is_empty(), "the work it was partway through is just above");
    let transcript = ui.transcript().unwrap();
    let measured = |ui: &Ui| ui.cx.read_entity(&transcript, |t, _| t.list().rows_to_measure() == 0);
    let newest = |ui: &Ui, own: fn(&EventBody) -> bool| ui.row_of(ui.events().iter().rfind(|e| own(&e.body)).unwrap());
    let failed = |body: &EventBody| matches!(body, EventBody::RequestFailed { .. });
    let picked_up = |body: &EventBody| matches!(body, EventBody::OwnTurn(_));

    for (times, task) in [(2, "first"), (3, "second")] {
        let answered = Gate::new(&stack, &format!("answered-{times}"));
        let card_before = newest(&ui, failed);
        // Moved into place whole, since the agent reads what it says as soon as it's there.
        let script = file(&format!("{task}.next"));
        std::fs::write(&script, format!("request-until:{answered}; fail:rate_limit")).unwrap();
        std::fs::rename(&script, file(task)).unwrap();
        ui.wait("the line of the turn it picked up in", |ui| ui.shown("ended-task").len() == 1);
        let line = newest(&ui, picked_up);
        ui.scroll("transcript", ScrollDelta::Pixels(point(px(0.), px(2_000.))));
        ui.wait("it out of sight", |ui| ui.shown("ended-task").is_empty() && ui.shown("failed-request").is_empty());

        answered.open();

        // The log is followed apart from the frame, and the list has a row past the log's for the chat's end, so it has
        // one past the failure's own only once the transcript has taken the failure in, and marked what it changes.
        let taken = |ui: &Ui| {
            let row = newest(ui, failed);
            ui.cx.read_entity(&transcript, |t, _| t.list().row_count() > row + 1)
        };
        ui.wait("the failure, and every height", |ui| failures(ui) == times && taken(ui) && measured(ui));
        let heights = ui.cx.read_entity(&transcript, |t, _| [card_before, line].map(|row| t.list().row_height(row)));
        assert_eq!(heights, [Some(px(0.)); 2], "what the card says takes no room where it was, out of sight");
        ui.keep_to_the_end();
        ui.wait("one card for them all", |ui| {
            ui.shown("failed-request").len() == 1 && ui.shown("failed-try").len() == times
        });
    }

    let newest_failure = ui.events().into_iter().rfind(|e| failed(&e.body)).unwrap();
    // Retry is offered once the chat's record says its turn failed, which comes on the hub's feed apart from the events.
    ui.wait("Retry on the card, which is the newest's", |ui| ui.visible(retry(newest_failure.seq)));
    assert!(ui.shown("ended-task").is_empty(), "each turn's line is in the card");
}
