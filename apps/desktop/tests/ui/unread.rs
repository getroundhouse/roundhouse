//! Chats with a reply nobody has read, which are read as they're shown in the window in front.

#![cfg(test)]

use client_core::Client;
use desktop::app::Connection;
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
    waiting::thread::until_ok(crate::support::TIMEOUT, || {
        let session = client.session_get(chat).unwrap();
        if session.has_reply() { Ok(session) } else { Err("timed out waiting for a reply") }
    })
}

fn read_at_the_hub(stack: &Stack, chat: &SessionId) -> bool {
    let session = stack.client().session_get(chat).unwrap();
    session.has_reply() && !session.unread()
}

fn unread_at_the_hub(stack: &Stack, chat: &SessionId) -> bool {
    stack.client().session_get(chat).unwrap().unread()
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

/// Marks the chat showing unread from its tab's menu, and waits for its tab and its workspace's row to say so.
fn mark_unread_from_its_tab(ui: &mut Ui, workspace: &WorkspaceId, chat: &SessionId) {
    ui.right_click(SharedString::from(format!("chat-{chat}")));
    ui.click(ElementId::Integer(0));
    ui.wait("its tab and row to say so", |ui| tab_unread(ui, chat) && row_unread(ui, workspace));
}

/// Somewhere the person goes from a chat, so a reply that comes to it meanwhile isn't seen.
#[derive(Clone, Copy, Debug)]
enum Away {
    InAnotherApp,
    InSettings,
    OnHome,
    InTheFileTab,
    InAnotherWorkspace,
}

impl Away {
    /// Goes there from the chat showing in `workspace`. Returns the workspace showing instead, if another is.
    fn go(self, ui: &mut Ui, workspace: &WorkspaceId) -> Option<WorkspaceId> {
        match self {
            Away::InAnotherApp => ui.send_to_back(),
            Away::InSettings => {
                ui.press("secondary-,");
                ui.wait("Settings", |ui| ui.has("settings-back"));
            }
            Away::OnHome => {
                ui.click("home");
                ui.wait("Home", |ui| ui.has("home-page"));
            }
            Away::InTheFileTab => {
                std::fs::write(ui.worktree(workspace).join("a.txt"), "one\ntwo\n").unwrap();
                let diff = ui.open_changes();
                ui.open_diff_of(&diff, "a.txt");
            }
            Away::InAnotherWorkspace => return Some(ui.new_workspace()),
        }
        None
    }

    /// Comes back to `chat` in `workspace` from there, as the person would.
    fn come_back(self, ui: &mut Ui, workspace: &WorkspaceId, chat: &SessionId) {
        match self {
            Away::InAnotherApp => ui.bring_to_front(),
            Away::InSettings => ui.press("escape"),
            Away::OnHome => ui.click(SharedString::from(format!("home-{workspace}"))),
            Away::InTheFileTab => ui.click(SharedString::from(format!("chat-{chat}"))),
            Away::InAnotherWorkspace => {
                ui.click(SharedString::from(format!("workspace-{workspace}")));
                ui.wait("it to show", |ui| ui.view().selected.as_ref() == Some(workspace));
            }
        }
    }
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

/// Wherever the person has gone from the chat, its tab and its workspace's row say it has a reply nobody has read
/// until they come back to it. Each place is gone to and come back from in turn, in one window.
#[gpui_kit::test]
fn a_reply_while_its_chat_isnt_in_front_is_unread_until_you_come_back_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);

    for away in [Away::InAnotherApp, Away::InSettings, Away::OnHome, Away::InTheFileTab, Away::InAnotherWorkspace] {
        let instead = away.go(&mut ui, &workspace);

        stack.client().session_prompt(&chat, &format!("say:while you were {away:?}"), None).unwrap();

        // Another workspace shows its own tabs, so only the row can say so.
        let said = |ui: &mut Ui| row_unread(ui, &workspace) && (instead.is_some() || tab_unread(ui, &chat));
        ui.wait(&format!("{away:?}: its tab and row to say so"), said);
        assert!(unread_at_the_hub(&stack, &chat), "{away:?}: the hub has it unread");
        if let Some(showing) = &instead {
            assert!(!row_unread(&ui, showing), "{away:?}: the workspace showing has nothing unread");
        }
        away.come_back(&mut ui, &workspace, &chat);
        ui.wait(&format!("{away:?}: the hub to have it read"), |_| read_at_the_hub(&stack, &chat));
        let gone = |ui: &mut Ui| !tab_unread(ui, &chat) && !row_unread(ui, &workspace);
        ui.wait(&format!("{away:?}: the marks to go"), gone);
    }
}

#[gpui_kit::test]
fn a_chat_marked_unread_stays_so_until_its_shown_again_even_across_a_hub_restart(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    ui.input("say:read this later");
    ui.press("enter");
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));

    mark_unread_from_its_tab(&mut ui, &workspace, &chat);

    assert!(unread_at_the_hub(&stack, &chat));
    for _ in 0..20 {
        ui.frame();
    }
    assert!(unread_at_the_hub(&stack, &chat), "showing it still doesn't read it");

    let pid = stack.client().hub_status().unwrap().pid;
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait_for_new_hub(&stack, pid);
    for _ in 0..50 {
        ui.frame();
    }

    assert!(unread_at_the_hub(&stack, &chat), "a hub restart doesn't read it");
    assert!(tab_unread(&ui, &chat) && row_unread(&ui, &workspace));
    // Going elsewhere and coming back still reads it, so it isn't unread only because nothing is read since.
    let other = ui.new_chat();
    ui.show_chat(&chat);
    assert_ne!(other, chat);
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
}

#[gpui_kit::test]
fn a_chat_marked_unread_stays_so_when_the_hub_is_lost_and_found_again(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    // The hub is started first, so the app finds it, and can't start one of its own once it goes.
    stack.client();
    let daemon = stack.without_a_daemon();
    let (mut ui, workspace, chat) = open_with_a_workspace(cx, &stack);
    ui.input("say:read this later");
    ui.press("enter");
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    mark_unread_from_its_tab(&mut ui, &workspace, &chat);

    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait("the app to find no hub", |ui| matches!(ui.view().connection(), Connection::Failed(_)));
    Client::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("the hub to be back", |ui| matches!(ui.view().connection(), Connection::Ready(_)));
    for _ in 0..50 {
        ui.frame();
    }

    assert!(unread_at_the_hub(&stack, &chat), "finding the hub again doesn't read it");
    ui.new_chat();
    ui.show_chat(&chat);
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &chat));
}

/// How a chat marked unread is shown again while the hub restarts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShownAgain {
    /// From another chat, which was showing.
    FromAnotherChat,
    /// By going to another chat and back, from the chat itself.
    ByGoingAwayAndBack,
}

/// Shown again before anything else runs, so the app is surely still connecting, and has no hub to mark it read on.
/// Each way of showing it again is tried in turn, in one window.
#[gpui_kit::test]
fn a_chat_marked_unread_and_shown_again_while_the_hub_restarts_is_read_once_its_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _, first) = open_with_a_workspace(cx, &stack);
    let later = ui.new_chat();
    ui.input("say:read this later");
    ui.press("enter");
    ui.wait("the hub to have it read", |_| read_at_the_hub(&stack, &later));

    for shown in [ShownAgain::FromAnotherChat, ShownAgain::ByGoingAwayAndBack] {
        ui.right_click(SharedString::from(format!("chat-{later}")));
        ui.click(ElementId::Integer(0));
        let unread = |ui: &mut Ui| unread_at_the_hub(&stack, &later) && tab_unread(ui, &later);
        ui.wait(&format!("{shown:?}: it to be unread, and its tab to say so"), unread);
        if shown == ShownAgain::FromAnotherChat {
            ui.show_chat(&first);
        }
        let (pane, pid) = (ui.pane().unwrap(), stack.client().hub_status().unwrap().pid);

        ui.cx.update(|cx| ui.view.update(cx, |view, cx| view.restart_hub(pid, cx)));
        ui.cx.update(|cx| {
            pane.update(cx, |p, cx| {
                if shown == ShownAgain::ByGoingAwayAndBack {
                    p.activate(first.clone(), cx);
                }
                p.activate(later.clone(), cx);
            });
        });

        assert!(matches!(ui.view().connection(), Connection::Connecting), "{shown:?}: still connecting");
        assert!(tab_unread(&ui, &later), "{shown:?}: nothing is read without a hub");
        ui.wait_for_new_hub(&stack, pid);
        ui.wait(&format!("{shown:?}: the hub to have it read"), |_| read_at_the_hub(&stack, &later));
    }
}

/// The sidebar's menu reads a workspace without showing it, and does what it offered when it opened. A reply in the
/// workspace while it's open is neither read unseen by "Mark as read", nor turns "Mark as unread" into reading it.
#[gpui_kit::test]
fn the_sidebars_menu_marks_a_workspace_without_showing_it_as_it_offered_when_it_opened(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, elsewhere, chat) = open_with_a_workspace(cx, &stack);
    let other = ui.new_chat();
    ui.new_workspace();
    stack.client().session_prompt(&chat, "say:the first", None).unwrap();
    ui.wait("its row to say so", |ui| row_unread(ui, &elsewhere));
    let first = replied(&stack, &chat).reply_seq;
    let menu = SharedString::from(format!("workspace-{elsewhere}"));

    // "Mark as read", with a second reply while the menu is open.
    ui.right_click(menu.clone());
    stack.client().session_prompt(&chat, "say:the second", None).unwrap();
    let mut second = first;
    ui.wait("Mark as read: the second reply", |_| {
        second = stack.client().session_get(&chat).unwrap().reply_seq;
        second > first
    });
    ui.click(ElementId::Integer(0));

    let first_read = |_: &mut Ui| stack.client().session_get(&chat).unwrap().read_seq >= first;
    ui.wait("Mark as read: the first reply to be read", first_read);
    let session = stack.client().session_get(&chat).unwrap();
    assert!(session.unread() && session.reply_seq == second, "Mark as read: the second reply was read unseen");

    // "Mark as read" again, with nothing coming, reads it all, and leaves the workspace where it is.
    ui.right_click(menu.clone());
    ui.click(ElementId::Integer(0));

    ui.wait("Mark as read again: the hub to have it read", |_| read_at_the_hub(&stack, &chat));
    ui.wait("Mark as read again: the mark to go", |ui| !row_unread(ui, &elsewhere));
    assert_ne!(ui.view().selected.as_ref(), Some(&elsewhere), "Mark as read again: it doesn't show the workspace");

    // "Mark as unread", with a reply in another of the workspace's chats while the menu is open.
    ui.right_click(menu);
    stack.client().session_prompt(&other, "say:while the menu is open", None).unwrap();
    ui.wait("Mark as unread: the other chat to be unread", |ui| row_unread(ui, &elsewhere));
    ui.click(ElementId::Integer(0));

    ui.wait("Mark as unread: the read chat to be unread again", |_| unread_at_the_hub(&stack, &chat));
    assert!(unread_at_the_hub(&stack, &other), "Mark as unread: the reply nobody has seen was read");
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
    assert!(unread_at_the_hub(&stack, &chat), "the hub has it unread too");

    ui.press("secondary-shift-u");

    ui.wait("it to be read again", |ui| !row_unread(ui, &workspace));
    ui.wait("the hub to have it read again", |_| read_at_the_hub(&stack, &chat));
}
