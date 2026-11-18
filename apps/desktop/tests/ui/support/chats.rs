//! A workspace's chats, and their tabs, and chats run through the hub before the app opens.

#![cfg(test)]

use gpui_kit::{AppContext as _, SharedString};
use protocol::capability::settings::Settings;
use protocol::event::{AgentRunState, EventBody, SessionEvent};
use protocol::model::{AgentKind, Place, Seq, SessionId, SessionStatus};

use super::{Stack, Ui, wait_until};

impl Stack {
    /// A chat in a new workspace in the stack's repository.
    pub(crate) fn chat_in_a_new_workspace(&self) -> SessionId {
        let client = self.client();
        let repo = client.repo_add(self.repo().to_str().unwrap()).unwrap();
        let workspace = client.workspace_create(&repo.id, None, None, None).unwrap();
        client.session_create(&workspace.id, AgentKind::Claude, Settings::default(), None).unwrap().id
    }

    /// The event the hub recorded at `seq` in the chat.
    pub(crate) fn event(&self, chat: &SessionId, seq: Seq) -> SessionEvent {
        self.client().session_read(chat, seq, 1, false).unwrap().0.remove(0)
    }

    /// The newest of the agent's own events in the chat, as the hub recorded it, which is the last the chat shows. The
    /// session's newest event can be a subagent's, which the chat doesn't follow.
    pub(crate) fn newest_event(&self, chat: &SessionId) -> SessionEvent {
        let client = self.client();
        let (_, last) = client.session_read_places(chat, None, Place::new(1), 1).unwrap();
        client.session_read_places(chat, None, last, 1).unwrap().0.remove(0)
    }

    /// Sends the script to the chat as a prompt, and waits for its agent to finish it.
    pub(crate) fn run(&self, chat: &SessionId, script: &str) {
        let client = self.client();
        client.session_prompt(chat, script, None).unwrap();
        self.wait_for_whole_turn(chat);
    }

    /// The fake agent says the prompt is done and then idle after the turn ends. Wait for that last event, since a
    /// test opening the transcript must not take the intervening, deliberately omitted `AgentOther` for its last row.
    pub(crate) fn wait_for_whole_turn(&self, chat: &SessionId) {
        wait_until("the agent to say it's idle, after the turn's end", || {
            self.client().session_get(chat).unwrap().status == SessionStatus::Idle
                && matches!(self.newest_event(chat).body, EventBody::AgentState { state: AgentRunState::Idle })
        });
    }
}

impl Ui<'_> {
    /// Has the chat showing run a command once its agent is free, with nothing asked, which takes it out of plan mode
    /// if it's in it. What the command changes shows when its turn ends. The fake agent takes `;` to start its next
    /// step, so the command has none.
    pub(crate) fn have_the_agent_run(&mut self, command: &str) {
        self.wait("the agent to be free", |ui| ui.session_status() == Some(SessionStatus::Idle));
        if self.chat_mode().is_some_and(|mode| mode.plan) {
            self.press("shift-tab");
            self.wait("full access", |ui| ui.chat_mode() == Some(protocol::fixtures::full_access()));
        }
        self.input(&format!("tool:{command}"));
        self.press("enter");
    }

    pub(crate) fn active_chat(&self) -> SessionId {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| p.active.clone().unwrap())
    }

    pub(crate) fn open_chats(&self) -> Vec<SessionId> {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| {
            p.sessions().iter().filter(|s| s.closed_at_ms.is_none()).map(|s| s.id.clone()).collect()
        })
    }

    /// Chooses a Claude chat from the `+` after the tabs and waits until that chat is showing. It's the one that wasn't
    /// open before, rather than the last, since chats are listed by when they were made, and two made within a
    /// millisecond of each other, or with the clock set back between them, needn't be listed in the order they were
    /// made.
    pub(crate) fn new_chat(&mut self) -> SessionId {
        let before = self.open_chats();
        self.pick_item("new-tab", "New Claude Code chat");
        self.wait("the new chat", |ui| ui.open_chats().len() > before.len() && ui.transcript().is_some());
        let newest = self.open_chats().into_iter().find(|id| !before.contains(id)).unwrap();
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
