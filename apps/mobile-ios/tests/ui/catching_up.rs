//! What the phone does while a new connection to its hub catches up on the hub's records. It opens on a fake hub that
//! holds back its list of records on a new connection until the test lets it go, so the window is held open.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{first_turn, text, user_prompt};
use gpui_kit::TestAppContext;
use mobile::MobileApp;
use protocol::model::SessionStatus;

use crate::support::{Phone, TIMEOUT};

/// A prompt sent while a new connection catches up isn't sent, as with no connection: it comes back to the composer,
/// and the chat says why. Once the connection has caught up, the phone is connected again.
#[gpui_kit::test]
fn a_prompt_sent_while_a_new_connection_catches_up_comes_back_to_the_composer(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::Idle, first_turn(user_prompt("Hello."), [text("hi", "Hi.")]));
    let workspace = world.workspaces.iter().find(|w| w.id == workspace).unwrap().clone();
    let (held, asked) = (world.held_list.clone(), world.asked.clone());
    let (_hub, mut phone) = Phone::on_a_fake_hub(cx, world);
    phone.open_chat(&workspace);
    held.hold();
    phone.cx.update(|cx| phone.app.update(cx, MobileApp::connect_again));
    waiting::thread::until(TIMEOUT, "the new connection's list to be held", || held.held() > 0);

    phone.send("say:after");

    phone.wait("the prompt back in the composer", |phone| phone.typed() == "say:after");
    assert_eq!(phone.chat_says().as_deref(), Some("Couldn't send that: the connection to the hub was lost"));
    assert_eq!(asked.count("session_prompt"), 0, "nothing was sent");
    held.let_go();
    phone.wait("the phone to be connected again", |phone| phone.is_connected());
}
