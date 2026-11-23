//! Pairing, the workspace list and a workspace's menu, starting a workspace, and forgetting the hub. A screen is known
//! by what only it has: `pair` for pairing, `confirm-pair` for the question before replacing the hub, `hub` for the
//! workspaces, `forget` for the hub's own screen, and `composer` for a chat. `chat.rs` has what's done in a chat.

#![cfg(test)]

use gpui_kit::{ElementId, TestAppContext};
use mobile::Vault as _;
use protocol::capability::settings::Settings;
use protocol::model::{AgentKind, Workspace, WorkspaceStatus};

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
    phone.wait("the hub's workspaces", |phone| phone.has("hub") && phone.is_connected());
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
    assert!(phone.has("hub"));
    assert_eq!(phone.vault.load().unwrap().device_id, first);

    phone.links.send(stack.pairing_link()).unwrap();
    phone.wait("the question", |phone| phone.has("confirm-pair"));
    phone.tap("confirm-pair");
    phone.wait("the new pairing", |phone| phone.vault.load().is_some_and(|hub| hub.device_id != first));
    phone.wait("its workspaces", |phone| phone.has("hub") && phone.is_connected());
}

#[gpui_kit::test]
fn pairs_from_a_pasted_link(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::unpaired(cx);

    phone.type_into("pairing-link", &stack.pairing_link());
    phone.tap("pair");

    phone.wait("the hub's workspaces", |phone| phone.has("hub") && phone.is_connected());
    assert!(phone.vault.load().is_some());
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

/// A repository's workspaces are listed last active first, as the desktop's sidebar lists them unless told otherwise,
/// so making a chat in the older one puts it on top.
#[gpui_kit::test]
fn lists_a_repositorys_workspaces_last_active_first(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (client, repo) = (stack.client(), stack.add_repo());
    let older = client.workspace_create(&repo.id, None, None, None).unwrap();
    let newer = client.workspace_create(&repo.id, None, None, None).unwrap();
    let mut phone = Phone::paired(cx, &stack);
    let row = |workspace: &Workspace| ElementId::Name(format!("workspace-{}", workspace.id).into());
    phone.wait("both workspaces", |phone| phone.has(row(&older)) && phone.has(row(&newer)));

    client.session_create(&older.id, AgentKind::Claude, Settings::default(), None).unwrap();

    let top = |phone: &mut Phone, workspace| phone.bounds(row(workspace)).origin.y;
    phone.wait("the older one on top", |phone| top(phone, &older) < top(phone, &newer));
}

/// A hub the phone couldn't forget would be followed again as the app next starts, so the phone stays with it until it
/// can.
#[gpui_kit::test]
fn forgetting_the_hub_goes_back_to_pairing_once_its_forgotten(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut phone = Phone::paired(cx, &stack);
    phone.wait("the connection", |phone| phone.is_connected());
    phone.tap("hub");
    assert!(phone.has("forget"));

    phone.vault.refuse_to_forget(true);
    phone.tap("forget");
    assert!(phone.has("forget") && phone.has("error"), "still on the hub's screen, saying why");
    assert!(phone.vault.load().is_some());

    phone.vault.refuse_to_forget(false);
    phone.tap("forget");
    assert!(phone.has("pair"));
    assert!(phone.vault.load().is_none());
}

/// A long press on a workspace's row opens its menu, and Archive asks first, in an action sheet, since the phone can't
/// bring a workspace back.
#[gpui_kit::test]
fn archives_a_workspace_from_its_menu_once_asked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    let row = ElementId::Name(format!("workspace-{}", workspace.id).into());
    phone.wait("the workspace", |phone| phone.has(row.clone()) && phone.is_connected());

    phone.press_long(row.clone());
    phone.choose("Archive");
    assert!(phone.has("action-sheet"), "it asks first");
    phone.tap("action-sheet-cancel");
    assert!(!phone.has("action-sheet") && phone.has(row.clone()), "cancelling leaves it");

    phone.press_long(row.clone());
    phone.choose("Archive");
    phone.tap("archive");

    phone.wait("the workspace to go from the list", |phone| !phone.has(row.clone()));
    let archived = stack.client().workspace_list(true).unwrap().into_iter().find(|w| w.id == workspace.id).unwrap();
    assert!(matches!(archived.status, WorkspaceStatus::Archiving | WorkspaceStatus::Archived));
}

/// A workspace with work archiving would lose isn't archived by the first Archive: the phone asks again, saying what
/// would be lost, and archives only once that's answered.
#[gpui_kit::test]
fn archiving_a_workspace_with_uncommitted_work_asks_again_before_losing_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    let row = ElementId::Name(format!("workspace-{}", workspace.id).into());
    phone.wait("the workspace", |phone| phone.has(row.clone()) && phone.is_connected());
    let status = || stack.client().workspace_list(true).unwrap().into_iter().find(|w| w.id == workspace.id).unwrap();
    let worktree = status().path.expect("a ready workspace has its worktree");
    std::fs::write(std::path::Path::new(&worktree).join("unsaved.txt"), "work in progress").unwrap();

    phone.press_long(row.clone());
    phone.choose("Archive");
    phone.tap("archive");
    phone.wait("it to ask again", |phone| phone.has("archive-anyway"));
    assert_eq!(status().status, WorkspaceStatus::Ready, "the first Archive doesn't force");

    phone.tap("archive-anyway");
    phone.wait("the workspace to go from the list", |phone| !phone.has(row.clone()));
    assert!(matches!(status().status, WorkspaceStatus::Archiving | WorkspaceStatus::Archived));
}

/// Renaming from a workspace's menu takes what's typed in a sheet.
#[gpui_kit::test]
fn renames_a_workspace_from_its_menu(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    let row = ElementId::Name(format!("workspace-{}", workspace.id).into());
    phone.wait("the workspace", |phone| phone.has(row.clone()) && phone.is_connected());

    phone.press_long(row.clone());
    phone.choose("Rename…");
    assert!(phone.has("renaming"));
    phone.type_text("Named on the phone");
    phone.tap("rename-confirm");

    phone.wait("the new name", |_| {
        stack.client().workspace_list(false).unwrap().iter().any(|w| w.label() == "Named on the phone")
    });
    assert!(!phone.has("renaming"));
}
