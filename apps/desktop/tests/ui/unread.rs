//! Chats with a reply nobody has read, which are read as they're shown in the window in front.

#![cfg(test)]

use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::model::{Session, SessionId, WorkspaceId};

use crate::support::{Stack, Ui};

/// Whether the sidebar shows this workspace as having a reply nobody has read.
fn row_unread(ui: &Ui, workspace: &WorkspaceId) -> bool {
    ui.cx.read_entity(&ui.view, |view, cx| !view.unread_chats(workspace, cx).is_empty())
}

/// Whether the workspace showing has a tab that shows this chat as unread.
fn tab_unread(ui: &Ui, chat: &SessionId) -> bool {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, |p, _| p.sessions().iter().any(|s| &s.id == chat && p.shows_unread(s)))
}

/// What the hub says of a chat, once it has a reply.
fn replied(stack: &Stack, chat: &SessionId) -> Session {
    let client = stack.client();
    let mut session = client.session_get(chat).unwrap();
    let deadline = std::time::Instant::now() + crate::support::TIMEOUT;
    while !session.has_reply() {
        assert!(std::time::Instant::now() < deadline, "timed out waiting for a reply");
        std::thread::sleep(std::time::Duration::from_millis(10));
        session = client.session_get(chat).unwrap();
    }
    session
}

fn read_at_the_hub(stack: &Stack, chat: &SessionId) -> bool {
    let session = stack.client().session_get(chat).unwrap();
    session.has_reply() && !session.unread()
}

/// A window in front, with a repository and a workspace whose chat is showing. Returns them.
fn open_with_a_workspace<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, WorkspaceId, SessionId) {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.bring_to_front();
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    let chat = ui.active_chat();
    (ui, workspace, chat)
}

#[gpui_kit::test]
fn a_reply_in_another_workspace_is_unread_until_its_shown(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, chat) = open_with_a_workspace(cx, &stack);
    let showing = ui.new_workspace();

    stack.client().session_prompt(&chat, "say:while you were away", None).unwrap();

    ui.wait("its row to say so", |ui| row_unread(ui, &elsewhere));
    assert!(replied(&stack, &chat).unread());
    assert!(!row_unread(&ui, &showing));
    ui.click(SharedString::from(format!("workspace-{elsewhere}")));
    ui.wait("it to show", |ui| ui.view().selected.as_ref() == Some(&elsewhere));
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    ui.wait("the mark to go", |ui| !row_unread(ui, &elsewhere) && !tab_unread(ui, &chat));
}

#[gpui_kit::test]
fn a_reply_in_the_chat_showing_is_read_as_it_comes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);

    ui.input("say:right here");
    ui.press("enter");

    ui.wait_idle_with("the reply", |text| text.contains("right here"));
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    assert!(!row_unread(&ui, &workspace) && !tab_unread(&ui, &chat));
}

#[gpui_kit::test]
fn a_reply_while_another_app_is_in_front_waits_for_the_app_to_come_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    ui.send_to_back();

    stack.client().session_prompt(&chat, "say:while you were in another app", None).unwrap();

    ui.wait("its tab and row to say so", |ui| tab_unread(ui, &chat) && row_unread(ui, &workspace));
    assert!(replied(&stack, &chat).unread());
    ui.bring_to_front();
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    ui.wait("the marks to go", |ui| !tab_unread(ui, &chat) && !row_unread(ui, &workspace));
}

#[gpui_kit::test]
fn a_reply_while_settings_shows_waits_until_you_go_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    ui.press("secondary-,");
    ui.wait("Settings", |ui| ui.has("settings-back"));

    stack.client().session_prompt(&chat, "say:while you were in Settings", None).unwrap();

    ui.wait("its tab and row to say so", |ui| tab_unread(ui, &chat) && row_unread(ui, &workspace));
    assert!(replied(&stack, &chat).unread());
    ui.press("escape");
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    ui.wait("the marks to go", |ui| !tab_unread(ui, &chat) && !row_unread(ui, &workspace));
}

#[gpui_kit::test]
fn a_reply_while_home_shows_waits_until_its_workspace_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));

    stack.client().session_prompt(&chat, "say:while you were in Home", None).unwrap();

    ui.wait("its tab and row to say so", |ui| tab_unread(ui, &chat) && row_unread(ui, &workspace));
    assert!(replied(&stack, &chat).unread());
    ui.click(SharedString::from(format!("home-{workspace}")));
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    ui.wait("the marks to go", |ui| !tab_unread(ui, &chat) && !row_unread(ui, &workspace));
}

#[gpui_kit::test]
fn a_reply_while_the_file_tab_shows_waits_until_the_chat_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");

    stack.client().session_prompt(&chat, "say:while you were reading the diff", None).unwrap();

    ui.wait("its tab and row to say so", |ui| tab_unread(ui, &chat) && row_unread(ui, &workspace));
    assert!(replied(&stack, &chat).unread());
    ui.click(SharedString::from(format!("chat-{chat}")));
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    ui.wait("the marks to go", |ui| !tab_unread(ui, &chat) && !row_unread(ui, &workspace));
}

#[gpui_kit::test]
fn a_chat_marked_unread_stays_so_until_its_shown_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    ui.input("say:read this later");
    ui.press("enter");
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));

    ui.right_click(SharedString::from(format!("chat-{chat}")));
    ui.click(ElementId::Integer(0));

    ui.wait("its tab and row to say so", |ui| tab_unread(ui, &chat) && row_unread(ui, &workspace));
    assert!(stack.client().session_get(&chat).unwrap().unread());
    for _ in 0..20 {
        ui.frame();
    }
    assert!(stack.client().session_get(&chat).unwrap().unread(), "showing it still doesn't read it");
    let other = ui.new_chat();
    ui.show_chat(&chat);
    assert_ne!(other, chat);
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
}

#[gpui_kit::test]
fn a_workspace_can_be_read_from_the_sidebar_without_showing_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, chat) = open_with_a_workspace(cx, &stack);
    ui.new_workspace();
    stack.client().session_prompt(&chat, "say:skim this", None).unwrap();
    ui.wait("its row to say so", |ui| row_unread(ui, &elsewhere));

    ui.right_click(SharedString::from(format!("workspace-{elsewhere}")));
    ui.click(ElementId::Integer(0));

    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    ui.wait("the mark to go", |ui| !row_unread(ui, &elsewhere));
    assert_ne!(ui.view().selected.as_ref(), Some(&elsewhere));
}

/// The sidebar's menu does what it offered when it opened. A reply in another of the workspace's chats while it's open
/// doesn't turn "Mark as unread" into reading that reply unseen.
#[gpui_kit::test]
fn mark_as_unread_still_does_that_when_a_reply_comes_while_the_menu_is_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, read) = open_with_a_workspace(cx, &stack);
    ui.input("say:read this");
    ui.press("enter");
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &read));
    let other = ui.new_chat();
    ui.new_workspace();
    ui.right_click(SharedString::from(format!("workspace-{elsewhere}")));

    stack.client().session_prompt(&other, "say:while the menu is open", None).unwrap();
    ui.wait("the other chat to be unread", |ui| row_unread(ui, &elsewhere));
    ui.click(ElementId::Integer(0));

    ui.wait("the read chat to be unread again", |_| stack.client().session_get(&read).unwrap().unread());
    assert!(stack.client().session_get(&other).unwrap().unread(), "the reply nobody has seen was read");
}

/// The same for "Mark as read": it reads what was there when the menu opened, and a reply since stays unread.
#[gpui_kit::test]
fn mark_as_read_leaves_a_reply_that_came_while_the_menu_was_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, chat) = open_with_a_workspace(cx, &stack);
    ui.new_workspace();
    stack.client().session_prompt(&chat, "say:the first", None).unwrap();
    ui.wait("its row to say so", |ui| row_unread(ui, &elsewhere));
    let first = replied(&stack, &chat).reply_seq;
    ui.right_click(SharedString::from(format!("workspace-{elsewhere}")));

    stack.client().session_prompt(&chat, "say:the second", None).unwrap();
    let mut second = first;
    ui.wait("the second reply", |_| {
        second = stack.client().session_get(&chat).unwrap().reply_seq;
        second > first
    });
    ui.click(ElementId::Integer(0));

    ui.wait("the first reply to be read", |_| stack.client().session_get(&chat).unwrap().read_seq >= first);
    let session = stack.client().session_get(&chat).unwrap();
    assert!(session.unread() && session.reply_seq == second, "the second reply was read unseen");
}

#[gpui_kit::test]
fn the_key_marks_the_workspace_showing_unread_and_read_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    ui.input("say:already read");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text.contains("already read"));
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));

    ui.press("secondary-shift-u");

    ui.wait("it to be unread", |ui| row_unread(ui, &workspace));
    assert!(stack.client().session_get(&chat).unwrap().unread(), "the hub has it unread too");

    ui.press("secondary-shift-u");

    ui.wait("it to be read again", |ui| !row_unread(ui, &workspace));
    ui.wait("the hub to have it read again", |_| read_at_the_hub(&stack, &chat));
}
