//! Conversation mode in the composer's place: its button turns it on once dictation and speech are ready, and until
//! then opens the card that sets them up, holding Space talks and letting go sends what was said, out loud, and typing
//! turns it off with what was typed in the composer.

use client_core::fake_hub::World;
use desktop::workspace_pane::{
    CONVERSATION_BAR, CONVERSATION_CARD, CONVERSATION_END, CONVERSATION_MODE, CONVERSATION_NOT_NOW, CONVERSATION_STOP,
    CONVERSATION_TURN_ON, LIVE_LINE, SPACE_DICTATES_AFTER,
};
use dictation::fake::FakeEngine;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{ClientDevice, Interaction, SessionStatus};

use super::holding_space::hold_for;
use super::support::{
    SCRIPT, dictating, ended, let_go, live, on_the_keyboard, press_space, ready, say, type_keys, use_model,
};
use crate::support::made_up_model::{made_up, put};
use crate::support::{FakeHub, Stack, Ui};

/// Has dictation ready, speech on with a voice that reads nothing, and the mode turned on in the chat showing.
fn in_conversation_mode<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, dictation::fake::FakeEngine) {
    in_conversation_mode_with(cx, stack, &[])
}

/// The same, with `typed` typed into the composer first, a key at a time.
fn in_conversation_mode_with<'a>(
    cx: &'a mut TestAppContext,
    stack: &Stack,
    typed: &[&'static str],
) -> (Ui<'a>, dictation::fake::FakeEngine) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    for key in typed {
        type_keys(&mut ui, key, key);
    }
    ui.cx.update(|cx| {
        desktop::conversing::use_a_silent_voice(cx);
        desktop::conversing::set_speech_on(true, cx);
    });
    ui.click(CONVERSATION_MODE);
    ui.wait("the bar to take the composer's place", |ui| ui.has(CONVERSATION_BAR));
    (ui, engine)
}

/// Whether the chat showing is in conversation mode, as the hub's record of it says.
fn on(ui: &mut Ui) -> bool {
    let pane = ui.pane().unwrap();
    let session = ui.cx.read_entity(&pane, |pane, _| pane.session().cloned()).unwrap();
    !matches!(session.interaction, Interaction::Typing)
}

#[gpui_kit::test]
fn the_button_turns_conversation_mode_on_and_typing_turns_it_off_with_what_was_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = in_conversation_mode(cx, &stack);
    assert!(on(&mut ui), "the hub's record says so");

    type_keys(&mut ui, "a", "a");
    type_keys(&mut ui, "b", "b");
    ui.wait("the composer to come back", |ui| !ui.has(CONVERSATION_BAR) && !on(ui));
    assert_eq!(ui.composer_text(), "ab", "with what was typed in it");
    assert_eq!(ui.view().error, None, "a key that asks again before the hub has turned it off isn't refused");
}

/// What was said, said out loud.
fn spoken(ui: &Ui) -> Option<String> {
    ui.events().iter().find_map(|e| match &e.body {
        EventBody::UserPrompt { prompt, .. } if prompt.spoken => Some(prompt.text.clone()),
        _ => None,
    })
}

#[gpui_kit::test]
fn letting_go_of_space_sends_what_was_said_out_loud(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, engine) = in_conversation_mode(cx, &stack);

    on_the_keyboard(&mut ui, true);
    press_space(&mut ui);
    hold_for(&mut ui, SPACE_DICTATES_AFTER);
    say(&mut ui, &engine, "fix the flaky test");
    on_the_keyboard(&mut ui, false);
    let_go(&mut ui);
    ended(&mut ui);

    ui.wait("what was said to be sent out loud", |ui| spoken(ui).is_some());
    assert!(ui.has(CONVERSATION_BAR), "the mode stays on");
    assert_eq!(ui.composer_text(), "", "nothing is left typed");
}

/// Something said that's longer than the bar stays on its one line, clear of what's at its right end.
#[gpui_kit::test]
fn something_long_said_stays_in_the_bar(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, engine) = in_conversation_mode(cx, &stack);

    on_the_keyboard(&mut ui, true);
    press_space(&mut ui);
    hold_for(&mut ui, SPACE_DICTATES_AFTER);
    // Sentences and pauses end what's heard so far, and the rest is still being heard.
    let said = format!("{}fix the flaky test", "fix the flaky test in the hub. / ".repeat(4));
    say(&mut ui, &engine, &said);
    ui.wait("all of what was said to show", |ui| {
        ui.composer_text().matches("hub.").count() == 4 && live(ui).is_some_and(|live| live.starts_with("fix the"))
    });

    // The words still being heard come last, so they're what anything too long pushes out.
    let (bar, esc, still) = (ui.bounds(CONVERSATION_BAR), ui.bounds(CONVERSATION_STOP), ui.bounds(LIVE_LINE));
    assert!(still.left() >= bar.left() && still.right() <= esc.left(), "{still:?} in {bar:?}, before {esc:?}");
    assert!(still.size.height < bar.size.height, "one line: {still:?} in {bar:?}");
    on_the_keyboard(&mut ui, false);
    let_go(&mut ui);
}

/// What was typed before the mode was turned on isn't sent with what's said, and is still there afterwards.
#[gpui_kit::test]
fn what_was_typed_before_is_not_said(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, engine) = in_conversation_mode_with(cx, &stack, &["n", "o", "t", "e"]);

    on_the_keyboard(&mut ui, true);
    press_space(&mut ui);
    hold_for(&mut ui, SPACE_DICTATES_AFTER);
    say(&mut ui, &engine, "fix the flaky test");
    on_the_keyboard(&mut ui, false);
    let_go(&mut ui);
    ended(&mut ui);

    ui.wait("what was said to be sent out loud", |ui| spoken(ui).is_some());
    assert_eq!(spoken(&ui).as_deref(), Some("Fix the flaky test."), "only what was said");
    assert_eq!(ui.composer_text(), "note", "what was typed is kept");
}

/// Has dictation hear with a model that's all here, so turning it on has it ready at once, without asking first, and
/// speech read with a voice that reads nothing, still asking first, as both do until they're chosen.
fn neither_on<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, FakeEngine) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    ui.new_workspace();
    let (files, bodies) = made_up(&["coreml/vocab.json"], 100);
    let models = stack.dir.path().join("models");
    put(&models, &files, &files.files[0], &bodies[0]);
    let engine = FakeEngine::new(SCRIPT);
    let loads = engine.clone();
    use_model(&mut ui, &models, files, "", move |_| Ok(loads.clone()));
    ui.cx.update(desktop::conversing::use_a_silent_voice);
    (ui, engine)
}

/// What the element `id` is called, as a screen reader says it.
fn label(ui: &mut Ui, id: &'static str) -> Option<String> {
    use gpui_kit::test::TestWindowExt as _;
    ui.frame();
    ui.cx
        .update_window(ui.handle, |_, window, _| window.try_find(id).and_then(|e| e.label().map(str::to_string)))
        .unwrap()
}

/// Turning the mode on before dictation and speech are ready opens the card rather than starting it, and Turn on
/// turns both on, after which the mode starts by itself.
#[gpui_kit::test]
fn the_mode_starts_once_the_card_has_dictation_and_speech_turned_on(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = neither_on(cx, &stack);

    ui.click(CONVERSATION_MODE);
    ui.wait("the card to open", |ui| ui.has(CONVERSATION_CARD));
    assert!(!on(&mut ui), "the mode isn't asked for before it can work");
    assert_eq!(label(&mut ui, CONVERSATION_TURN_ON).as_deref(), Some("Turn on both"));

    ui.click(CONVERSATION_TURN_ON);
    ui.wait("the mode to start by itself", |ui| ui.has(CONVERSATION_BAR) && on(ui));
    assert!(!ui.has(CONVERSATION_CARD), "the card is done with");
}

/// Not now closes the card, and dictation and speech getting ready after that doesn't start the mode.
#[gpui_kit::test]
fn not_now_closes_the_card_and_nothing_starts(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = neither_on(cx, &stack);

    ui.click(CONVERSATION_MODE);
    ui.wait("the card to open", |ui| ui.has(CONVERSATION_CARD));
    ui.click(CONVERSATION_NOT_NOW);
    ui.wait("the card to close", |ui| !ui.has(CONVERSATION_CARD));

    ui.cx.update(|cx| {
        desktop::dictating::set_on(true, cx);
        desktop::conversing::set_speech_on(true, cx);
    });
    ui.wait("dictation and speech to be ready", |ui| ui.cx.update(|cx| desktop::conversing::is_ready(cx)));
    ui.frame();
    assert!(!on(&mut ui) && !ui.has(CONVERSATION_BAR), "the mode stays off");
}

/// With the conversation here and dictation not on, as when it came from another device first, the bar says so,
/// rather than saying to hold Space, and holding Space sends nothing.
#[gpui_kit::test]
fn holding_space_does_nothing_until_dictation_is_ready(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, engine) = in_conversation_mode(cx, &stack);
    ui.cx.update(|cx| desktop::dictating::set_on(false, cx));
    // Turn on, in the bar, which says to turn dictation on rather than to hold Space.
    ui.wait("the bar to say what's missing", |ui| ui.has(CONVERSATION_TURN_ON));

    on_the_keyboard(&mut ui, true);
    press_space(&mut ui);
    hold_for(&mut ui, SPACE_DICTATES_AFTER);
    assert!(!dictating(&mut ui), "nothing is heard");
    say(&mut ui, &engine, "fix the flaky test");
    on_the_keyboard(&mut ui, false);
    let_go(&mut ui);
    ui.frame();

    assert!(spoken(&ui).is_none(), "nothing is said");
    assert_eq!(ui.composer_text(), "", "no space is typed");
    assert!(ui.has(CONVERSATION_BAR), "the mode stays on");
}

/// With the conversation here and dictation ready but speech not, holding Space sends nothing, since the bar waits for
/// speech rather than showing what's heard.
#[gpui_kit::test]
fn holding_space_sends_nothing_until_speech_is_ready_too(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, engine) = in_conversation_mode(cx, &stack);
    ui.cx.update(|cx| desktop::conversing::set_speech_on(false, cx));
    ui.wait("the bar to say what's missing", |ui| ui.has(CONVERSATION_TURN_ON));

    on_the_keyboard(&mut ui, true);
    press_space(&mut ui);
    hold_for(&mut ui, SPACE_DICTATES_AFTER);
    assert!(!dictating(&mut ui), "nothing is heard");
    say(&mut ui, &engine, "fix the flaky test");
    on_the_keyboard(&mut ui, false);
    let_go(&mut ui);
    ui.frame();

    assert!(spoken(&ui).is_none(), "nothing is said");
    assert!(ui.has(CONVERSATION_BAR), "the mode stays on");
}

/// `⇧⌘Space` where the card can't show, as with dictation off, which takes the mode's button away, does nothing, and
/// dictation and speech getting ready later doesn't start the mode.
#[gpui_kit::test]
fn the_key_does_nothing_where_the_card_cant_show(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = neither_on(cx, &stack);
    ui.cx.update(|cx| desktop::dictating::set_on(false, cx));
    ui.wait("the mode's button to go", |ui| !ui.has(CONVERSATION_MODE));

    ui.cx.update(|cx| cx.dispatch_action(&desktop::ConversationMode));
    ui.frame();
    assert!(!ui.has(CONVERSATION_CARD), "no card");

    ui.cx.update(|cx| {
        desktop::dictating::set_on(true, cx);
        desktop::conversing::set_speech_on(true, cx);
    });
    ui.wait("dictation and speech to be ready", |ui| ui.cx.update(|cx| desktop::conversing::is_ready(cx)));
    ui.frame();
    assert!(!on(&mut ui) && !ui.has(CONVERSATION_BAR), "the mode stays off");
}

/// A hub that won't turn the mode off says why, and the conversation carries on, as the mode is still on, so ✕ asks
/// again as it did the first time. The fake hub refuses every change.
#[gpui_kit::test]
fn the_hub_refusing_to_turn_the_mode_off_says_why_and_it_can_be_asked_again(cx: &mut TestAppContext) {
    let this_mac = ClientDevice { id: "this-mac".into(), name: "This Mac".into() };
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    let started = EventBody::ConversationStarted { device: this_mac.clone() };
    let chat = world.chat(&workspace, SessionStatus::Idle, vec![started]);
    let session = world.sessions.iter_mut().find(|session| session.id == chat).unwrap();
    session.interaction = Interaction::Conversation { device: Box::new(this_mac.clone()) };
    let fake = FakeHub::holding(world);
    std::fs::write(fake.source().paths.data_dir.join("device-id"), this_mac.id.to_string()).unwrap();
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the bar, since the conversation is on this Mac", |ui| ui.has(CONVERSATION_BAR));
    ui.wait("the conversation to be here", here);

    for asking in ["first", "again"] {
        ui.click(CONVERSATION_END);
        ui.wait(&format!("the refusal, asking {asking}"), |ui| ui.view().error.is_some());
        assert_eq!(
            ui.view().error.as_deref(),
            // As a hub too old to have conversation mode refuses, which is how the fake one refuses a change.
            Some("Couldn't turn conversation mode off: this hub can't do that until it's updated")
        );
        assert!(ui.has(CONVERSATION_BAR), "the mode is still on, asking {asking}");
        ui.wait(&format!("the conversation to carry on, asking {asking}"), here);
        ui.click("dismiss-error");
    }
}

/// Whether the conversation of the chat showing is on this Mac, so it hears and reads.
fn here(ui: &mut Ui) -> bool {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, desktop::workspace_pane::WorkspacePane::conversation_is_here)
}
