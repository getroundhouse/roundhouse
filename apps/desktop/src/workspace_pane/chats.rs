//! Which chats a workspace has, and which one is showing.

use gpui_kit::{AppContext, Context, PromptLevel, Result, Window};
use protocol::model::{AgentKind, Session, SessionId};
use transcript::{Transcript, TranscriptEvent};

use super::{PaneEvent, WorkspacePane};
use crate::diff_panel::DiffPanel;
use client_core::records::Record;

/// What a new chat starts with. It keeps the model and how edits are handled, but Plan belongs to the
/// task at hand, and full access should be chosen on purpose each time.
pub(super) fn inherited_mode(mode: Option<&str>) -> Option<String> {
    mode.filter(|mode| !matches!(*mode, "plan" | "bypassPermissions")).map(String::from)
}

/// The chat after this one, or else the one before.
pub(super) fn neighbor(open: &[SessionId], position: usize) -> Option<SessionId> {
    open.get(position).or_else(|| open.last()).cloned()
}

impl WorkspacePane {
    /// For a workspace with no open chat. Another client, or whoever is making this workspace, may be
    /// about to give it one, so this joins that rather than making a second.
    pub(super) fn ensure_chat(&mut self, cx: &mut Context<Self>) {
        if self.creating_chat {
            return;
        }
        self.creating_chat = true;
        let id = self.workspace.id.clone();
        let finding = self.client.start(move |client| client.session_ensure(&id, AgentKind::Claude));
        cx.spawn(async move |this, cx| {
            let found = finding.recv().await;
            let _ = this.update(cx, |t, cx| t.chat_created(found, cx));
        })
        .detach();
    }

    /// Starts another chat in this workspace and shows it.
    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        if self.creating_chat {
            return;
        }
        self.creating_chat = true;
        let id = self.workspace.id.clone();
        let model = self.session().and_then(|s| s.model.clone());
        let mode = inherited_mode(self.session().and_then(|s| s.permission_mode.as_deref()));
        let creating = self.client.start(move |client| {
            client.session_create(&id, AgentKind::Claude, model, mode, Some(client_core::new_request_id()))
        });
        cx.spawn(async move |this, cx| {
            let created = creating.recv().await;
            let _ = this.update(cx, |t, cx| t.chat_created(created, cx));
        })
        .detach();
    }

    fn chat_created(&mut self, created: Result<Session, client_core::ClientError>, cx: &mut Context<Self>) {
        self.creating_chat = false;
        match created {
            Ok(session) => self.took_chat_to_show(session, cx),
            Err(e) => cx.emit(PaneEvent::Error(format!("couldn't start a chat: {e}"))),
        }
    }

    /// Shows a chat, keeping what was being typed in the one it replaces.
    pub fn activate(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if self.active.as_ref() == Some(&id) {
            return;
        }
        if let Some(current) = self.active.take() {
            let draft = self.composer.read(cx).text().to_string();
            if draft.is_empty() {
                self.drafts.remove(&current);
            } else {
                self.drafts.insert(current, draft);
            }
        }
        if !self.transcripts.contains_key(&id)
            && let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned()
        {
            self.make_transcript(session, cx);
        }
        self.active = Some(id);
        self.show_draft(cx);
        self.read_showing(cx);
        cx.notify();
    }

    /// This needs the window, and a click handler is already inside a window update, so it runs just
    /// after whatever called it.
    fn show_draft(&mut self, cx: &mut Context<Self>) {
        let (handle, this) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |t, cx| {
                    let draft = t.active.as_ref().and_then(|id| t.drafts.remove(id)).unwrap_or_default();
                    let end = draft.len();
                    t.composer.update(cx, |c, cx| {
                        c.set_value(draft, window, cx);
                        c.set_selected_range(end..end, cx);
                        c.focus(window, cx);
                    });
                });
            });
        });
    }

    fn make_transcript(&mut self, session: Session, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let view = cx.new(|cx| {
            let mut transcript = Transcript::new(client, &session, cx);
            transcript.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx);
            transcript
        });
        let events = cx.subscribe(&view, |this, _, event: &TranscriptEvent, cx| match event {
            TranscriptEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            TranscriptEvent::TurnEnded => {
                this.refresh_pull_request(cx);
                if let Some(diff) = &this.diff {
                    diff.update(cx, DiffPanel::refresh);
                }
            }
        });
        self.transcripts.insert(session.id, (view, events));
    }

    /// Moves to the chat before or after this one, wrapping around.
    pub fn step_chat(&mut self, step: isize, cx: &mut Context<Self>) {
        let open: Vec<SessionId> = self.open_sessions().map(|s| s.id.clone()).collect();
        let Some(current) = open.iter().position(|id| Some(id) == self.active.as_ref()) else { return };
        let next = crate::app::wrapped_step(current, step, open.len());
        self.activate(open[next].clone(), cx);
    }

    /// Asks first if it would stop an agent that's working.
    pub fn close_chat(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        let working = self.sessions.iter().any(|s| s.id == id && s.status.is_active());
        if !working {
            self.finish_closing(id, cx);
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Close this chat?",
            Some("Its agent is still working, and closing the chat stops it. The history stays."),
            &["Close chat", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update(cx, |t, cx| t.finish_closing(id, cx));
            }
        })
        .detach();
    }

    fn finish_closing(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let closing = self.client.start(move |client| client.session_close(&id));
        cx.spawn(async move |this, cx| {
            let closed = closing.recv().await;
            let _ = this.update(cx, |t, cx| match closed {
                Ok(session) => t.took_reply(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't close the chat: {e}"))),
            });
        })
        .detach();
    }

    pub fn reopen_chat(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let reopening = self.client.start(move |client| client.session_reopen(&id));
        cx.spawn(async move |this, cx| {
            let reopened = reopening.recv().await;
            let _ = this.update(cx, |t, cx| match reopened {
                Ok(session) => t.took_chat_to_show(session, cx),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't reopen the chat: {e}"))),
            });
        })
        .detach();
    }

    /// Passes on a session a reply carried, for client-core's copy, which this then shows like any other
    /// change.
    pub fn took_reply(&mut self, session: Session, cx: &mut Context<Self>) {
        self.records.take_reply(Record::Session(session));
        self.show_next(cx);
    }

    /// The same, and the chat becomes the one showing once client-core's copy has it open.
    fn took_chat_to_show(&mut self, session: Session, cx: &mut Context<Self>) {
        self.showing_next = Some(session.id.clone());
        self.took_reply(session, cx);
    }

    pub(super) fn set_sessions(&mut self, sessions: Vec<Session>, cx: &mut Context<Self>) {
        self.sessions = sessions;
        self.keep_active_valid(cx);
        self.show_next(cx);
        self.read_showing(cx);
        cx.notify();
    }

    /// Shows the chat waiting to be shown, once client-core's copy has it open.
    fn show_next(&mut self, cx: &mut Context<Self>) {
        let ready = self.showing_next.as_ref().is_some_and(|id| self.open_sessions().any(|s| &s.id == id));
        if ready && let Some(id) = self.showing_next.take() {
            self.activate(id, cx);
        }
    }

    /// Moves off a chat that's been closed, here or anywhere else, and starts one if none are left.
    fn keep_active_valid(&mut self, cx: &mut Context<Self>) {
        let closed: Vec<SessionId> =
            self.sessions.iter().filter(|s| s.closed_at_ms.is_some()).map(|s| s.id.clone()).collect();
        for id in &closed {
            self.transcripts.remove(id);
            self.drafts.remove(id);
        }
        let open: Vec<SessionId> = self.open_sessions().map(|s| s.id.clone()).collect();
        if self.active.as_ref().is_some_and(|id| open.contains(id)) {
            return;
        }
        // Where the closed one was, counting only the chats before it that are still open.
        let position = self
            .active
            .as_ref()
            .and_then(|active| self.sessions.iter().position(|s| &s.id == active))
            .map_or(0, |at| self.sessions[..at].iter().filter(|s| s.closed_at_ms.is_none()).count());
        self.active = None;
        if let Some(id) = neighbor(&open, position) {
            self.activate(id, cx);
        } else {
            self.show_draft(cx);
            self.ensure_chat(cx);
        }
    }
}
