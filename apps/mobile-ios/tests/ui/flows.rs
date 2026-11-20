//! Pairing, the workspace list, a chat, starting a workspace, and forgetting the hub. A screen is known by what only it
//! has: `pair` for pairing, `confirm-pair` for the question before replacing the hub, `hub` for the workspaces, `forget`
//! for the hub's own screen, and `composer` for a chat.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::{ElementId, TestAppContext};
use mobile::Vault as _;
use protocol::capability::settings::Settings;
use protocol::model::AgentKind;

use crate::support::{Phone, Stack, id};

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

/// A workspace opens on the chat it has, which another client made, rather than on one of the phone's own.
#[gpui_kit::test]
fn opens_a_listed_workspace_on_the_chat_it_has_and_talks_in_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let chat = stack.client().session_create(&workspace.id, AgentKind::Claude, Settings::default(), None).unwrap();
    let mut phone = Phone::paired(cx, &stack);

    phone.open_chat(&workspace);
    phone.send("say:hello from the phone");

    phone.wait("the agent's answer", |phone| phone.assistant_text().contains("hello from the phone"));
    let sessions = stack.client().session_list(&workspace.id).unwrap();
    assert_eq!(sessions.iter().map(|s| &s.id).collect::<Vec<_>>(), [&chat.id], "the phone made no chat of its own");
    phone.tap("back");
    assert!(phone.has("hub"));
}

/// The chat's header names the workspace as its record does, which another client can change while it's open.
#[gpui_kit::test]
fn the_chats_header_names_the_workspace_as_its_record_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    assert_eq!(phone.chat_title().as_deref(), Some(workspace.label()));

    stack.client().workspace_rename(&workspace.id, "Renamed on the desktop").unwrap();

    phone.wait("the new title", |phone| phone.chat_title().as_deref() == Some("Renamed on the desktop"));
}

/// Whether the agent is working is the session's record's, which the chat takes as it changes, and the working line
/// it ends with says so.
#[gpui_kit::test]
fn the_chat_says_the_agent_is_working_until_its_turn_ends(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    assert!(!phone.has("working-line"));

    let done = stack.gate("done");
    phone.send(&format!("hang-until:{}; say:done", done.display()));
    phone.wait("the working line", |phone| phone.has("working-line"));

    std::fs::write(&done, "").unwrap();
    phone.wait("the turn to end", |phone| phone.assistant_text().contains("done") && !phone.has("working-line"));
}

/// The phone can't put a chat in plan mode, which asks before a call that writes, so the desktop did.
#[gpui_kit::test]
fn allows_a_tool_use_in_the_composers_place(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let planning = Settings { mode: Some(protocol::fixtures::plan_mode()), ..Settings::default() };
    stack.client().session_create(&workspace.id, AgentKind::Claude, planning, None).unwrap();
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);

    phone.send("say:starting; tool:echo changed >> a.txt; say:finished");
    phone.next_open_approval();
    phone.wait("the request to be asked", |phone| phone.has(id("question-answer-0")));
    assert!(!phone.has("composer"), "asked in the composer's place");
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\n", "not until it's allowed");
    phone.tap(id("question-answer-0"));
    phone.tap(id("question-send"));

    phone.wait("the turn to finish", |phone| phone.assistant_text().contains("finished"));
    assert_eq!(std::fs::read_to_string(worktree.join("a.txt")).unwrap(), "one\nchanged\n");
    assert!(phone.has("composer"), "the composer is back");
}

#[gpui_kit::test]
fn answers_a_question_by_typing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let workspace = stack.workspace();
    let mut phone = Phone::paired(cx, &stack);
    phone.open_chat(&workspace);
    phone.send("ask:Which day?=Monday/Tuesday");
    phone.next_open_approval();
    phone.wait("the question", |phone| phone.has(id("question-other")));

    phone.tap(id("question-other"));
    phone.type_text("Friday, if that works");
    phone.tap(id("question-send"));

    phone.wait("the typed answer", |phone| phone.told_to_agent().contains(r#""Which day?"=Friday, if that works"#));
}

/// The list's connection, and a chat open on it, both carry on over the new one.
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
