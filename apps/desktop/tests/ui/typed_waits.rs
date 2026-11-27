//! What's typed in a rename, or in a queued prompt's editor, waits while a new connection catches up on the hub's
//! records, as with no connection: Rename is off and Enter does nothing, so what's typed stays where it is, and the
//! same press goes through once the connection has caught up. Each opens on a fake hub that holds back its list of
//! records on a new connection until the test lets it go.

#![cfg(test)]

use client_core::fake_hub::World;
use gpui_kit::{ElementId, SharedString, TestAppContext};
use protocol::model::{QueuedPrompt, SessionStatus};
use protocol::prompt::{Prompt, PromptFrom};

use crate::catching_up::{connect_again, throughout};
use crate::support::{FakeHub, Ui, one_chat, open_on_the_chat};

/// Right-clicks `id` and picks Rename…, then types `title` in place of what the dialog starts with.
fn start_renaming(ui: &mut Ui<'_>, id: &str, title: &str) {
    ui.right_click(SharedString::from(id.to_string()));
    ui.click_label("Rename…");
    ui.input(title);
}

/// Holds the next connection's list back, and connects again, so it's held catching up.
fn catching_up(ui: &mut Ui<'_>, held: &client_core::fake_hub::HeldList) {
    held.hold();
    connect_again(ui);
    crate::support::wait_until("the new connection's list to be held", || held.held() > 0);
}

/// While it catches up, Rename is off and Enter keeps the dialog open, with nothing asked of the hub. Once it has,
/// Rename comes on, and Enter renames to what was typed.
fn a_rename_waits(
    ui: &mut Ui<'_>,
    world_asked: &client_core::fake_hub::Asked,
    held: &client_core::fake_hub::HeldList,
    method: &str,
) {
    throughout(ui, "Rename stays off while the connection catches up", |ui| ui.off("rename-confirm"));
    ui.press("enter");

    assert!(ui.has_dialog(), "Enter left the dialog open");
    assert_eq!(world_asked.count(method), 0, "nothing was asked");
    held.let_go();
    ui.wait("Rename to come on once it has caught up", |ui| !ui.off("rename-confirm"));
    ui.press("enter");
    ui.wait("the dialog to close", |ui| !ui.has_dialog());
    // It's asked on a thread of its own, so the hub may not have it yet as the dialog closes.
    crate::support::wait_until("the hub to be asked to rename it", || world_asked.count(method) > 0);
    let asked = world_asked.last(method).expect("the rename was asked");
    assert_eq!(asked["title"], "Kept while away", "what was typed while the hub was away");
}

#[gpui_kit::test]
fn a_chat_s_rename_keeps_what_s_typed_while_a_new_connection_catches_up(cx: &mut TestAppContext) {
    let (world, chat) = one_chat(SessionStatus::Idle, Vec::new());
    let (held, asked) = (world.held_list.clone(), world.asked.clone());
    let (_hub, mut ui) = open_on_the_chat(cx, world, &chat);
    start_renaming(&mut ui, &format!("chat-{chat}"), "Kept while away");
    catching_up(&mut ui, &held);

    a_rename_waits(&mut ui, &asked, &held, "session_rename");
}

#[gpui_kit::test]
fn a_workspace_s_rename_keeps_what_s_typed_while_a_new_connection_catches_up(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspace = world.workspace(&repo, "ledger");
    world.chat(&workspace, SessionStatus::Idle, Vec::new());
    let (held, asked) = (world.held_list.clone(), world.asked.clone());
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the workspace", |ui| ui.pane().is_some());
    start_renaming(&mut ui, &format!("workspace-{workspace}"), "Kept while away");
    catching_up(&mut ui, &held);

    a_rename_waits(&mut ui, &asked, &held, "workspace_rename");
}

/// While it catches up, Enter in a queued prompt's editor leaves it open with what's typed, and nothing is asked of
/// the hub. Once it has, Enter changes the prompt to what was typed.
#[gpui_kit::test]
fn a_queued_prompt_s_edit_keeps_what_s_typed_while_a_new_connection_catches_up(cx: &mut TestAppContext) {
    let (mut world, chat) = one_chat(SessionStatus::Idle, Vec::new());
    let prompt =
        QueuedPrompt { client_msg_id: "queued-1".into(), prompt: Prompt::new("say:first"), from: PromptFrom::Person };
    world.sessions[0].queued = vec![prompt.clone()];
    let (held, asked) = (world.held_list.clone(), world.asked.clone());
    let (_hub, mut ui) = open_on_the_chat(cx, world, &chat);
    let id = |name: &str| ElementId::Name(SharedString::from(format!("{name}-{}", prompt.client_msg_id)));
    ui.wait("the queue", |ui| ui.has(id("queued")));
    ui.hover(id("queued"));
    ui.click(id("edit-queued"));
    ui.press("secondary-a");
    ui.input("say:kept while away");
    catching_up(&mut ui, &held);

    ui.press("enter");
    // The prompt's text shows in place of the editor once it closes.
    throughout(&mut ui, "the editor stays open", |ui| !ui.has(id("queued")));
    assert_eq!(asked.count("session_change_queued"), 0, "nothing was asked");
    held.let_go();
    ui.wait("the pane to be back on the hub", |ui| !ui.off("send"));
    ui.press("enter");

    ui.wait("the editor to close", |ui| ui.has(id("queued")));
    // It's asked on a thread of its own, so the hub may not have it yet as the editor closes.
    crate::support::wait_until("the hub to be asked for the change", || asked.count("session_change_queued") > 0);
    let changed = asked.last("session_change_queued").expect("the change was asked");
    assert_eq!(changed["text"], "say:kept while away", "what was typed while the hub was away");
}
