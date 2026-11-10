//! How full the agent's context is: the ring beside Send, and the card it shows while the pointer rests on it.

#![cfg(test)]

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::ContextUsage;

use crate::support::{Stack, Ui};

/// How full the record of the chat showing says its agent's context is.
fn context(ui: &Ui<'_>) -> Option<ContextUsage> {
    let pane = ui.pane()?;
    ui.cx.read_entity(&pane, |pane, _| pane.session().and_then(|session| session.context.clone()))
}

/// What the ring says to assistive apps, which is how full it's drawn.
fn ring_says(ui: &mut Ui<'_>, said: &str) -> bool {
    ui.labelled(said).iter().any(|ring| ring.path().last().is_some_and(|id| *id == "context".into()))
}

/// A new workspace whose agent has taken one turn, and said how full its context is.
fn after_a_turn<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    ui.new_workspace();
    assert!(!ui.has("context"), "a chat whose agent has said nothing has no ring");
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));
    ui.wait("the ring", |ui| ui.has("context"));
    ui
}

#[gpui_kit::test]
fn a_chat_has_a_ring_once_its_agent_says_how_full_its_context_is_and_the_ring_follows_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_a_turn(cx, &stack);

    // The fake agent's context holds 15,000 tokens before anything is said, and 1,000 more for each prompt, of 200,000.
    ui.wait("the agent to say how full its context is", |ui| context(ui).is_some_and(|c| c.used_tokens == 16_000));
    assert!(ring_says(&mut ui, "Context 8.0% used"));

    ui.input("say:more");
    ui.press("enter");
    ui.wait_idle_with("the next turn to finish", |text| text.contains("more"));

    ui.wait("the ring to follow", |ui| ring_says(ui, "Context 8.5% used"));
}

#[gpui_kit::test]
fn the_card_shows_once_the_pointer_has_rested_on_the_ring_and_goes_as_soon_as_it_leaves(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_a_turn(cx, &stack);

    ui.hover("context");
    assert!(!ui.has("context-card"), "passing over the ring shows nothing");
    ui.wait_out_a_tooltip();
    assert!(ui.visible("context-card"), "resting on it does");

    // No time passes here, so the card going is the pointer's doing.
    ui.hover("new-chat");
    ui.wait("the card to go", |ui| !ui.has("context-card"));
}

#[gpui_kit::test]
fn the_card_fills_in_the_account_and_its_limits_once_the_hub_has_said(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_a_turn(cx, &stack);

    ui.hover("context");
    ui.wait_out_a_tooltip();

    // The hub reads the account from the worker once the card shows: who's logged in, and the week's limit the agent
    // heard of as it worked.
    ui.wait("the account", |ui| ui.visible("context-account"));
    ui.wait("the account's limits", |ui| ui.visible("context-limits"));
}
