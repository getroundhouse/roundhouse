//! Pairing, the workspace list, a chat, starting a workspace, and forgetting the hub. A screen is known by what
//! only it has: `pair` for pairing, `confirm-pair` for the question before replacing the hub, `hub-info` for the
//! workspaces, `forget` for the hub's own screen, and `composer` for a chat.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::{ElementId, TestAppContext};
use mobile::Vault as _;

use crate::support::{Phone, Stack};

#[gpui_kit::test]
fn opens_on_pairing_and_pairs_from_a_link_the_phone_opens(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::unpaired(cx);
    assert!(phone.has("pair"));

    phone.links.send("roundhouse://pair?not-a-real-link".into()).unwrap();
    phone.wait("pairing to fail", |phone| phone.has("pairing-failed"));
    assert!(phone.vault.load().is_none());

    // What the Camera app opens.
    phone.links.send(stack.pairing_link()).unwrap();
    phone.wait("the hub's workspaces", |phone| phone.has("hub-info") && phone.is_connected());
    let paired = phone.vault.load().unwrap();
    let devices = stack.client().device_list().unwrap();
    assert_eq!(devices.iter().map(|d| (&d.id, d.name.as_str())).collect::<Vec<_>>(), [(&paired.device_id, "iPhone")]);
}

#[gpui_kit::test]
fn a_link_opened_while_paired_asks_before_replacing_the_hub(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::paired(cx, &stack);
    phone.wait("the connection", |phone| phone.is_connected());
    let first = phone.vault.load().unwrap().device_id;

    // Any app can open a link, so one arriving changes nothing until it's confirmed.
    phone.links.send(stack.pairing_link()).unwrap();
    phone.wait("the question", |phone| phone.has("confirm-pair"));
    phone.tap("keep-hub");
    assert!(phone.has("hub-info"));
    assert_eq!(phone.vault.load().unwrap().device_id, first);

    phone.links.send(stack.pairing_link()).unwrap();
    phone.wait("the question", |phone| phone.has("confirm-pair"));
    phone.tap("confirm-pair");
    phone.wait("the new pairing", |phone| phone.vault.load().is_some_and(|hub| hub.device_id != first));
    phone.wait("its workspaces", |phone| phone.has("hub-info") && phone.is_connected());
}

#[gpui_kit::test]
fn pairs_from_a_pasted_link(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::unpaired(cx);

    phone.type_into("pairing-link", &stack.pairing_link());
    phone.tap("pair");

    phone.wait("the hub's workspaces", |phone| phone.has("hub-info") && phone.is_connected());
    assert!(phone.vault.load().is_some());
}

#[gpui_kit::test]
fn lists_the_hubs_workspaces_and_chats_in_one(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);

    phone.open_chat(&workspace);
    phone.send("say:hello from the phone");

    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("hello from the phone"));
    let sessions = stack.client().session_list(&workspace.id).unwrap();
    assert_eq!(sessions.len(), 1, "the phone joined the workspace's chat rather than making another");
    phone.tap("back");
    assert!(phone.has("hub-info"));
}

#[gpui_kit::test]
fn answers_an_approval_in_the_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);

    phone.send("say:starting; tool:echo changed >> a.txt; say:finished");
    let mut approval = None;
    phone.wait("the approval", |phone| {
        approval = phone.open_approval();
        approval.is_some()
    });
    let allow = ElementId::NamedInteger("allow".into(), approval.unwrap());
    phone.wait("the approval's buttons", |phone| phone.has(allow.clone()));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n", "not until it's allowed");
    phone.tap(allow);

    phone.wait("the turn to finish", |phone| phone.assistant_text().contains("finished"));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\nchanged\n");
}

#[gpui_kit::test]
fn typing_while_the_agent_asks_answers_its_question(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    phone.send("ask:Which day?=Monday/Tuesday");
    phone.wait("the question", |phone| phone.open_approval().is_some());

    phone.send("Friday, if that works");

    phone.wait("the typed answer", |phone| phone.told_to_agent().contains(r#""Which day?"=Friday, if that works"#));
}

#[gpui_kit::test]
fn a_chat_carries_on_after_the_hub_restarts(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);

    stack.stop_hub();
    phone.wait("the connection to go", |phone| !phone.is_connected());
    stack.start_hub();
    phone.wait("the connection to come back", |phone| phone.is_connected());
    phone.send("say:after the restart");

    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("after the restart"));
    assert!(phone.has("composer"), "still in the chat");
}

#[gpui_kit::test]
fn starts_a_workspace_and_opens_its_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let repo = stack.add_repo();
    let mut phone = Phone::paired(cx, &stack);
    phone.wait("the connection", |phone| phone.is_connected());

    phone.tap("new-workspace");
    let row = ElementId::Name(format!("repo-{}", repo.id).into());
    phone.wait("the repository", |phone| phone.has(row.clone()));
    phone.tap(row);

    phone.wait("the new workspace's chat", |phone| phone.has("composer") && phone.transcript_shown());
    assert_eq!(stack.client().workspace_list(false).unwrap().len(), 1);
    phone.send("say:started from the phone");
    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("started from the phone"));
}

#[gpui_kit::test]
fn connects_again_when_the_hub_comes_back(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    let mut phone = Phone::paired(cx, &stack);
    phone.wait("the connection", |phone| phone.is_connected());

    stack.stop_hub();
    phone.wait("the connection to go", |phone| !phone.is_connected());
    stack.start_hub();

    phone.wait("the connection to come back", |phone| phone.is_connected());
}

#[gpui_kit::test]
fn forgetting_the_hub_goes_back_to_pairing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::paired(cx, &stack);
    phone.wait("the connection", |phone| phone.is_connected());

    phone.tap("hub-info");
    assert!(phone.has("forget"));
    phone.tap("forget");

    assert!(phone.has("pair"));
    assert!(phone.vault.load().is_none());
}
