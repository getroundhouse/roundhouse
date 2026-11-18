//! Beside Send: the ring for how full the agent's context is, the bars for the account's limits, and the card each
//! shows while the pointer rests on it.

#![cfg(test)]

use client_core::fake_hub::events::{replied, taken, text, user_prompt};
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::event::EventBody;
use protocol::fixtures::account;
use protocol::model::{Account, ContextUsage, LimitWindow, MaxSetBy, Seq, SessionStatus, UsageLimit};

use crate::support::{Gate, Stack, Ui, one_chat, open_on_the_chat};

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
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));
    ui.wait("the ring", |ui| ui.has("context"));
    ui
}

/// The ring and the bars follow the chat's record and the account's, as the hub changes them from what the agent says,
/// with nothing in the app asking for either, so the bars come even while another app is in front.
#[gpui_kit::test]
fn the_ring_and_the_limits_bars_follow_what_the_agent_says_even_while_another_app_is_in_front(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    assert!(!ui.has("context"), "a chat whose agent has said nothing has no ring");
    ui.input("say:hello");
    ui.send_to_back();

    // The agent hears of the week's limit as it works, with nobody looking.
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));

    ui.wait("the limits' bars", |ui| ui.has("usage"));
    assert!(
        ui.labelled("Claude usage 96% left")
            .iter()
            .any(|bars| bars.path().last().is_some_and(|id| *id == "usage".into()))
    );
    // The fake agent's context holds 15,000 tokens before anything is said, and 1,000 more for each prompt, of 200,000.
    ui.wait("the agent to say how full its context is", |ui| context(ui).is_some_and(|c| c.used_tokens == 16_000));
    assert!(ring_says(&mut ui, "Context 8.0% used"));
    ui.bring_to_front();
    ui.input("say:more");
    ui.press("enter");
    ui.wait_idle_with("the next turn to finish", |text| text.contains("more"));
    ui.wait("the ring to follow", |ui| ring_says(ui, "Context 8.5% used"));
}

/// Whether the chat's log holds a request to compact that said `keeping`, and the compaction that answers it.
fn compacted(ui: &Ui<'_>, keeping: Option<&str>) -> bool {
    let events = ui.events();
    let asked =
        events.iter().find(|e| matches!(&e.body, EventBody::CompactionAsked { keeping: k } if k.as_deref() == keeping));
    asked.is_some_and(|asked| {
        events
            .iter()
            .any(|e| matches!(e.body, EventBody::AgentCompacted { asked: Some(named), .. } if named == asked.seq))
    })
}

/// The card's button and `/compact` ask the same way, so a compaction is recorded and drawn the same whichever way it's
/// asked, and what follows `/compact` is what to keep. Neither can ask while the agent is on a turn: the button is off,
/// and what's typed is refused and stays typed.
#[gpui_kit::test]
fn the_cards_button_and_typing_compact_ask_the_agent_to_compact_except_while_it_is_on_a_turn(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_a_turn(cx, &stack);
    ui.hover("context");
    ui.wait_out_a_tooltip();
    assert!(ui.on("context-compact"));

    // Off the ring, and onto the card, which waits for the pointer to cross.
    ui.click("context-compact");

    ui.wait("the agent to compact, as the button asked", |ui| compacted(ui, None));
    let seq = ui.events().iter().find(|e| matches!(e.body, EventBody::AgentCompacted { .. })).map(|e| e.seq).unwrap();
    let rule = format!("compaction/event/{seq}/0");
    ui.wait("the chat to draw it", |ui| ui.has(gpui_kit::SharedString::from(rule.clone())));

    ui.click("composer");
    ui.input("/compact Keep the greeting");
    ui.press("enter");

    ui.wait("the agent to compact, as typed", |ui| compacted(ui, Some("Keep the greeting")));
    assert_eq!(ui.composer_text(), "");
    let typed = ui.events().iter().any(|e| matches!(e.body, EventBody::UserPrompt { .. } if e.seq > Seq::new(2)));
    assert!(!typed, "it isn't a prompt");

    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("hang-until:{done}"));
    ui.press("enter");
    ui.wait("the agent to start working", |ui| ui.session_status() == Some(SessionStatus::Running));
    ui.input("/compact");
    ui.press("enter");

    ui.wait("it to come back", |ui| ui.composer_text() == "/compact");
    assert!(ui.view().error.as_deref().is_some_and(|error| error.starts_with("Couldn't compact")));
    ui.hover("context");
    ui.wait_out_a_tooltip();
    assert!(ui.off("context-compact"), "the card's button is off while the agent is on a turn");
}

/// A chat whose agent has said how full its context is, a fifth of what its model holds, as the agent's settings make
/// it, under an account whose agent has heard of the week's limit, as the fake agent says them after a turn.
fn after_a_turn_on_a_fake_hub(cx: &mut TestAppContext) -> (crate::support::FakeHub, Ui<'_>) {
    let events = vec![user_prompt("say:hello"), taken(Seq::new(1)), text("msg-1", "hello"), replied("hello", 2, 0.01)];
    let (mut world, chat) = one_chat(SessionStatus::Idle, events);
    let week = UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 4,
        // Shown by any clock, which here is the machine's.
        resets_at_ms: None,
        heard_at_ms: None,
    };
    let account = Account {
        email: Some("ada@example.com".to_string()),
        limits: vec![week],
        ..account("claude-0000000000000001")
    };
    let session = world.sessions.last_mut().unwrap();
    session.context = Some(ContextUsage {
        used_tokens: 16_000,
        max_tokens: 200_000,
        model_tokens: Some(1_000_000),
        max_set_by: Some(MaxSetBy::Settings),
        parts: Vec::new(),
    });
    session.account = Some(account.id.clone());
    world.accounts.push(account);
    open_on_the_chat(cx, world, &chat)
}

/// Each card shows once the pointer has rested on what it's for, and the ring's, which holds a button, goes only once
/// the pointer could have reached it. The ring's says only the context, and what made it smaller than its model's,
/// since the limits' card says whose account it is.
#[gpui_kit::test]
fn each_card_shows_once_the_pointer_rests_and_the_rings_says_only_the_context_and_the_limits_whose_they_are(
    cx: &mut TestAppContext,
) {
    let (_fake, mut ui) = after_a_turn_on_a_fake_hub(cx);
    // The agent heard of the week's limit as it worked, which the account's record says, so the bars come without the
    // pointer going near them.
    ui.wait("the ring and the limits' bars", |ui| ui.has("context") && ui.has("usage"));

    ui.hover("context");
    assert!(!ui.has("context-card"), "passing over the ring shows nothing");
    ui.wait_out_a_tooltip();
    assert!(ui.visible("context-card"), "resting on it does");
    assert!(ui.has("context-capped"), "the ring's card says what capped the context");
    assert!(!ui.has("account"), "the limits' card says whose account it is");
    assert!(!ui.has("usage-card"), "the ring's card is only the context");

    // It holds a button, so it waits for the pointer to cross to it.
    ui.hover("new-tab");
    assert!(ui.visible("context-card"), "the pointer could still be on its way");
    ui.wait_out_a_tooltip();
    ui.wait("the ring's card to go", |ui| !ui.has("context-card"));

    ui.hover("usage");
    ui.wait_out_a_tooltip();
    assert!(ui.visible("usage-card"), "resting on the bars shows their card");
    assert!(ui.visible("account"), "which says whose account it is");
}
