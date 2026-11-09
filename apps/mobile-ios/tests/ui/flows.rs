//! Pairing, the workspace list, a chat, starting a workspace, and forgetting the hub.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::{ElementId, SharedString, TestAppContext};
use mobile::Vault as _;

use crate::support::{Phone, Stack};

#[gpui_kit::test]
fn opens_on_pairing_and_pairs_from_a_link_the_phone_opens(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::open(cx, &stack, false);
    assert_eq!(phone.showing(), "pairing");
    assert!(phone.has("pair"));

    phone.links.send("roundhouse://pair?not-a-real-link".into()).unwrap();
    phone.wait("pairing to fail", |phone| phone.showing() == "pairing" && phone.vault.load().is_none());

    // What the Camera app opens.
    let link = stack.pairing_link();
    phone.links.send(link).unwrap();
    phone.wait("the hub's workspaces", |phone| phone.showing() == "workspaces" && phone.is_connected());
    let paired = phone.vault.load().unwrap();
    let devices = stack.client().device_list().unwrap();
    assert_eq!(devices.iter().map(|d| d.id.to_string()).collect::<Vec<_>>(), [paired.device_id]);
    assert_eq!(devices[0].name, "iPhone");
}

#[gpui_kit::test]
fn pairs_from_a_pasted_link(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::open(cx, &stack, false);

    phone.type_into("pairing-link", &stack.pairing_link());
    phone.tap("pair");

    phone.wait("the hub's workspaces", |phone| phone.showing() == "workspaces" && phone.is_connected());
    assert!(phone.vault.load().is_some());
}

#[gpui_kit::test]
fn lists_the_hubs_workspaces_and_chats_in_one(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
    let mut phone = Phone::open(cx, &stack, true);
    let row = SharedString::from(format!("workspace-{}", workspace.id));
    phone.wait("the workspace", |phone| phone.has(row.clone()));

    phone.tap(row);
    phone.wait("its chat", |phone| phone.showing() == "chat" && phone.has("composer"));
    phone.type_into("composer", "say:hello from the phone");
    phone.tap("send");

    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("hello from the phone"));
    let sessions = client.session_list(&workspace.id).unwrap();
    assert_eq!(sessions.len(), 1, "the phone joined the workspace's chat rather than making another");

    phone.tap("back");
    assert_eq!(phone.showing(), "workspaces");
}

#[gpui_kit::test]
fn answers_an_approval_in_the_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    let mut phone = Phone::open(cx, &stack, true);
    let row = SharedString::from(format!("workspace-{}", workspace.id));
    phone.wait("the workspace", |phone| phone.has(row.clone()));
    phone.tap(row);
    phone.wait("its chat", |phone| phone.has("composer"));

    phone.type_into("composer", "say:starting; tool:echo changed >> a.txt; say:finished");
    phone.tap("send");
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
fn starts_a_workspace_and_opens_its_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let repo = stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut phone = Phone::open(cx, &stack, true);
    phone.wait("the connection", |phone| phone.is_connected());

    phone.tap("new-workspace");
    assert_eq!(phone.showing(), "new workspace");
    let row = SharedString::from(format!("repo-{}", repo.id));
    phone.wait("the repository", |phone| phone.has(row.clone()));
    phone.tap(row);

    phone.wait("the new workspace's chat", |phone| phone.showing() == "chat" && phone.has("composer"));
    let workspaces = stack.client().workspace_list(false).unwrap();
    assert_eq!(workspaces.len(), 1);
    phone.type_into("composer", "say:started from the phone");
    phone.tap("send");
    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("started from the phone"));
}

#[gpui_kit::test]
fn connects_again_when_the_hub_comes_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::open(cx, &stack, true);
    phone.wait("the connection", |phone| phone.is_connected());

    stack.stop_hub();
    phone.wait("the connection to go", |phone| !phone.is_connected());
    stack.start_hub();

    phone.wait("the connection to come back", |phone| phone.is_connected());
}

#[gpui_kit::test]
fn forgetting_the_hub_goes_back_to_pairing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::open(cx, &stack, true);
    phone.wait("the connection", |phone| phone.is_connected());

    phone.tap("hub-info");
    assert_eq!(phone.showing(), "hub info");
    phone.tap("forget");

    assert_eq!(phone.showing(), "pairing");
    assert!(phone.vault.load().is_none());
}
