//! What the window does while a new connection to its hub catches up on the hub's records: nothing a chat asks is sent
//! until then, and what it couldn't do meanwhile is done once it has. Each opens on a fake hub that holds back its list
//! of records on a new connection until the test lets it go, so the window is held open.

#![cfg(test)]

use std::time::{Duration, Instant};

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, first_turn, text, turn, user_prompt};
use desktop::app::AppView;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::attachment::{Attachment, MediaType};
use protocol::event::EventBody;
use protocol::model::{AttachmentId, SessionStatus};
use protocol::prompt::{Prompt, PromptFrom};

use crate::support::{Ui, one_chat, open_on_the_chat};

/// Long enough for the app to take up a new connection it has been told of, which it does at once.
const LONG_ENOUGH: Duration = Duration::from_millis(300);
/// Longer than the find bar waits for typing to pause.
const PAUSE: Duration = Duration::from_secs(1);

/// Connects again now, as Try again does, so the next connection is held catching up.
fn connect_again(ui: &mut Ui<'_>) {
    let view = ui.view.clone();
    ui.cx.update(|cx| view.update(cx, AppView::connect));
}

/// For as long as the app could take to take up a new connection, checks `holds` holds on each frame.
fn throughout(ui: &mut Ui<'_>, what: &str, mut holds: impl FnMut(&mut Ui<'_>) -> bool) {
    let until = Instant::now() + LONG_ENOUGH;
    while Instant::now() < until {
        ui.frame();
        assert!(holds(ui), "{what}");
    }
}

/// While the new connection catches up, Send is off and Enter leaves what's typed where it is, as with no hub, so
/// nothing is sent that the hub's first list may be about to make wrong. Once it has, Send is on again.
#[gpui_kit::test]
fn the_composer_waits_until_a_new_connection_has_caught_up(cx: &mut TestAppContext) {
    let (world, chat) = one_chat(SessionStatus::Idle, first_turn(user_prompt("Hello."), [text("hi", "Hi.")]));
    let held = world.held_list.clone();
    let asked = world.asked.clone();
    let (_hub, mut ui) = open_on_the_chat(cx, world, &chat);
    ui.input("say:after");
    assert!(!ui.off("send"), "Send is on while the hub is here");
    held.hold();

    connect_again(&mut ui);
    crate::support::wait_until("the new connection's list to be held", || held.held() > 0);
    throughout(&mut ui, "Send stays off while the connection catches up", |ui| ui.off("send"));
    ui.press("enter");

    assert_eq!(ui.composer_text(), "say:after", "Enter left what's typed");
    held.let_go();
    ui.wait("Send to be on once it has caught up", |ui| !ui.off("send"));
    assert_eq!(asked.count("session_prompt"), 0, "nothing was sent");
}

/// A picture a chat couldn't read, and a search it couldn't make, while a new connection caught up, are asked for again
/// once it has, rather than kept as failed.
#[gpui_kit::test]
fn what_a_chat_couldn_t_read_or_find_while_catching_up_is_asked_again_once_it_has(cx: &mut TestAppContext) {
    let shot = Attachment {
        id: AttachmentId::from("shot-1"),
        name: "shot.png".into(),
        media_type: MediaType::Png,
        size_bytes: 3,
    };
    let mut world = World::default();
    world.attachments.insert(shot.id.clone(), crate::attachments::png());
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    // Far more events than a chat loads to start with, so the whole chat is searched at the hub. What's found is in the
    // last reply, beside the picture, so the chat stays where the picture is drawn.
    let mut events = Vec::new();
    let said = Vec::from_fn(1000, |i| text(&format!("count-{i}"), &format!("message {i}")));
    turn(&mut events, user_prompt("Count."), said);
    let looked_at = EventBody::UserPrompt {
        client_msg_id: None,
        prompt: Prompt::carrying("Look.".to_string(), vec![shot.clone()]),
        from: PromptFrom::Person,
    };
    turn(&mut events, looked_at, [text("seen", "Seen."), finished_turn(3, 0.01)]);
    let second = world.chat(&workspace, SessionStatus::Idle, events);
    // Made last, so it's the chat last worked in, which the window opens on, and the other's picture isn't read before.
    let first = world.chat(&workspace, SessionStatus::Idle, first_turn(user_prompt("Hello."), [text("hi", "Hi.")]));
    let (held, asked) = (world.held_list.clone(), world.asked.clone());
    let (_hub, mut ui) = open_on_the_chat(cx, world, &first);
    held.hold();
    connect_again(&mut ui);
    crate::support::wait_until("the new connection's list to be held", || held.held() > 0);

    ui.show_chat(&second);
    let transcript = ui.transcript().unwrap();
    let unreadable = |ui: &mut Ui<'_>| ui.cx.read_entity(&transcript, |t, _| t.picture_unreadable(&shot.id));
    ui.wait("the picture, unreadable", unreadable);
    ui.cx.update_window(ui.handle, |_, window, cx| transcript.update(cx, |t, cx| t.open_find(window, cx))).unwrap();
    ui.input("Seen");
    ui.cx.executor().advance_clock(PAUSE);
    ui.wait("the search to fail", |ui| ui.view().error.as_deref().is_some_and(|e| e.starts_with("Couldn't find")));
    let (read_then, found_then) = (asked.count("attachment_read"), asked.count("session_find"));
    held.let_go();

    ui.wait("the picture, read once it has caught up", |_| asked.count("attachment_read") > read_then);
    ui.wait("the search, asked again", |_| asked.count("session_find") > found_then);
    ui.wait("the picture, shown", |ui| !unreadable(ui));
}
