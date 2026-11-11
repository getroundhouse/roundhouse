//! Beside Send: the ring for how full the agent's context is, the bars for the account's limits, and the card each
//! shows while the pointer rests on it.

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
fn the_accounts_limits_have_bars_of_their_own_once_its_agent_has_heard_them(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_a_turn(cx, &stack);

    // The agent heard of the week's limit as it worked, which the account's record says, so the bars come without the
    // pointer going near them.
    ui.wait("the limits' bars", |ui| ui.has("usage"));

    ui.hover("usage");
    ui.wait_out_a_tooltip();
    assert!(ui.visible("usage-card"), "resting on the bars shows their card");
    assert!(ui.visible("account"), "which says whose account it is");
}

#[gpui_kit::test]
fn the_rings_card_is_only_the_context_while_the_limits_have_a_card_of_their_own(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_a_turn(cx, &stack);
    ui.wait("the limits' bars", |ui| ui.has("usage"));

    ui.hover("context");
    ui.wait_out_a_tooltip();

    assert!(ui.visible("context-card"));
    assert!(!ui.has("account"), "the limits' card says whose account it is");
    assert!(!ui.has("usage-card"));
}

#[gpui_kit::test]
fn the_limits_bars_follow_the_accounts_record_even_while_another_app_is_in_front(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.input("say:hello");
    ui.send_to_back();

    // The agent hears of the week's limit as it works, with nobody looking. Nothing in the app asks for it: the
    // account's record says so, and the feed brings it.
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));

    ui.wait("the limits' bars", |ui| ui.has("usage"));
    assert!(
        ui.labelled("Claude usage 96% left")
            .iter()
            .any(|bars| bars.path().last().is_some_and(|id| *id == "usage".into()))
    );
}
