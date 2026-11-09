//! A workspace's chats, and their tabs.

#![cfg(test)]

use gpui_kit::{AppContext as _, SharedString};
use protocol::model::SessionId;

use super::Ui;

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

    /// Presses a chat's close button, the way someone would: with the pointer over its tab first, since that's
    /// when it shows.
    pub(crate) fn close_from_tab(&mut self, chat: &SessionId) {
        self.hover(SharedString::from(format!("chat-{chat}")));
        self.click(SharedString::from(format!("close-chat-{chat}")));
    }
}
