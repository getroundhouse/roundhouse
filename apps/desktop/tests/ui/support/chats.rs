//! A workspace's chats, and their tabs, and chats run through the hub before the app opens.

#![cfg(test)]

use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, SharedString};
use protocol::model::{AgentKind, SessionId, SessionOptions, SessionStatus};

use super::{Stack, TIMEOUT, Ui};

impl Stack {
    /// A chat whose agent doesn't ask, in a new workspace in the stack's repository.
    pub(crate) fn chat_in_full_access(&self) -> SessionId {
        let client = self.client();
        let repo = client.repo_add(self.repo().to_str().unwrap()).unwrap();
        let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
        let options = SessionOptions { permission_mode: Some("bypassPermissions".into()), ..SessionOptions::default() };
        client.session_create(&workspace.id, AgentKind::Claude, options, None).unwrap().id
    }

    /// Sends the script to the chat as a prompt, and waits for its agent to finish it.
    pub(crate) fn run(&self, chat: &SessionId, script: &str) {
        let client = self.client();
        client.session_prompt(chat, script, None).unwrap();
        let deadline = Instant::now() + TIMEOUT;
        while client.session_get(chat).unwrap().status != SessionStatus::Idle {
            assert!(Instant::now() < deadline, "the agent never finished {script}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Ui<'_> {
    pub(crate) fn active_chat(&self) -> SessionId {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| p.active.clone().unwrap())
    }

    pub(crate) fn open_chats(&self) -> Vec<SessionId> {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| {
            p.sessions.iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.id.clone()).collect()
        })
    }

    /// Presses the new chat button and waits until that chat is showing.
    pub(crate) fn new_chat(&mut self) -> SessionId {
        let before = self.open_chats().len();
        self.click("new-chat");
        self.wait("the new chat", |ui| ui.open_chats().len() > before && ui.transcript().is_some());
        let newest = self.open_chats().last().unwrap().clone();
        self.wait("the new chat to show", |ui| ui.active_chat() == newest);
        newest
    }

    pub(crate) fn show_chat(&mut self, id: &SessionId) {
        self.click(SharedString::from(format!("chat-{id}")));
        self.wait("the chat to show", |ui| ui.active_chat() == *id);
    }

    /// Shows the chat without the pointer or a key, as another client's change can, so nothing under the pointer sees
    /// the chat it was over go.
    pub(crate) fn activate_chat(&mut self, id: &SessionId) {
        let pane = self.pane().unwrap();
        self.cx.update_entity(&pane, |p, cx| p.activate(id.clone(), cx));
        self.wait("the chat to show", |ui| ui.active_chat() == *id);
    }

    /// Presses a chat's close button, the way someone would: with the pointer over its tab first, since that's
    /// when it shows.
    pub(crate) fn close_from_tab(&mut self, chat: &SessionId) {
        self.hover(SharedString::from(format!("chat-{chat}")));
        self.click(SharedString::from(format!("close-chat-{chat}")));
    }
}
