//! Which chats a workspace has, which one is showing, and what each is called.

use std::time::Duration;

use client_core::ClientError;
use client_core::records::InWorkspace;
use gpui_kit::{AppContext, Context, Result, Window};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Session, SessionId};
use transcript::{Transcript, TranscriptEvent};

use super::attaching::Composed;
use super::{Chat, Next, PaneEvent, Showing, WorkspacePane};
use crate::asking::{self, Question};
use crate::prompt_cache;
use crate::rename::{self, Renaming};

/// What to show once `gone`, which was open, isn't: the first chat after it that's still open, or else the last open.
/// It counts from the chats open before, since one closed before anything was sent in it is deleted, and so is in
/// neither list now.
pub(super) fn neighbor(was_open: &[SessionId], gone: &SessionId, open: &[SessionId]) -> Option<SessionId> {
    let at = was_open.iter().position(|id| id == gone).unwrap_or(0);
    let still_open_before = was_open[..at].iter().filter(|id| open.contains(id)).count();
    open.get(still_open_before).or_else(|| open.last()).cloned()
}

impl WorkspacePane {
    /// For a workspace with no open chat. Another client, or whoever is making this workspace, may be
    /// about to give it one, so this joins that rather than making a second.
    pub(super) fn ensure_chat(&mut self, cx: &mut Context<Self>) {
        if self.creating_chat {
            return;
        }
        self.creating_chat = true;
        let (id, agent) = (self.workspace().id.clone(), self.new_chat_agent());
        let finding = self.client.start(move |client| client.session_ensure(&id, agent));
        cx.spawn(async move |this, cx| {
            let found = finding.recv().await;
            let _ = this.update(cx, |t, cx| t.chat_created(found, cx));
        })
        .detach();
    }

    /// Starts another chat in this workspace and shows it.
    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        let agent = self.session().map_or_else(|| self.new_chat_agent(), |s| s.agent);
        self.new_chat_with(agent, cx);
    }

    /// Every request makes a chat, as `Cmd-T` opens a tab every time elsewhere. One made while another is on its way
    /// waits for it.
    pub(super) fn new_chat_with(&mut self, agent: protocol::model::AgentKind, cx: &mut Context<Self>) {
        if self.creating_chat {
            self.chats_asked_for.push_back(agent);
            return;
        }
        self.creating_chat = true;
        let id = self.workspace().id.clone();
        let options = client_core::new_chat_settings(self.session(), agent);
        let creating = self
            .client
            .start(move |client| client.session_create(&id, agent, options, Some(client_core::new_request_id())));
        cx.spawn(async move |this, cx| {
            let created = creating.recv().await;
            let _ = this.update(cx, |t, cx| t.chat_created(created, cx));
        })
        .detach();
    }

    fn chat_created(&mut self, created: Result<Session, client_core::ClientError>, cx: &mut Context<Self>) {
        self.creating_chat = false;
        match created {
            Ok(session) => self.show_chat_once_open(session.id, cx),
            Err(e) => cx.emit(PaneEvent::Error(client_core::refusals::couldnt("start a chat", &e))),
        }
        if let Some(agent) = self.chats_asked_for.pop_front() {
            self.new_chat_with(agent, cx);
        }
    }

    /// Shows a chat, keeping what was being typed in the one it replaces.
    pub fn activate(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if self.active.as_ref() == Some(&id) {
            self.show_chat(cx);
            return;
        }
        self.end_dictation(cx);
        if let Some(current) = self.active.take() {
            let text = self.composer.read(cx).text().to_string();
            let draft = Composed { text, files: std::mem::take(&mut self.attached) };
            let kept = !draft.text.is_empty() || !draft.files.is_empty();
            self.held(&current).draft = kept.then_some(draft);
        }
        if self.chats.get(&id).is_none_or(|chat| chat.transcript.is_none())
            && let Some(session) = self.shown.session(&id).cloned()
        {
            self.make_transcript(session, cx);
        }
        self.active = Some(id);
        self.wind_cache_clock(cx);
        self.show_draft(cx);
        self.show_chat(cx);
    }

    /// Brings the active chat to the front, in place of the file tab, with what was typed in it still there.
    pub(super) fn show_chat(&mut self, cx: &mut Context<Self>) {
        self.bring_to_front(Showing::Chat, cx);
    }

    /// Puts the active chat's draft in the composer, once there's the window to do it with, or for a chat with none, a
    /// prompt waiting for the first chat to show, and plan mode if it was to be planned. Files can come for the chat
    /// between its being made active and this, such as with a prompt handed back, and they go after the draft's.
    fn show_draft(&mut self, cx: &mut Context<Self>) {
        self.with_window(cx, |t, window, cx| {
            let draft = t.active.as_ref().and_then(|id| t.chats.get_mut(id)?.draft.take());
            let mut arrived = false;
            let draft = draft.or_else(|| {
                let waiting = t.waiting.take()?;
                if let Some(chat) = t.active.clone().filter(|_| waiting.to_plan) {
                    t.want_plan_mode(chat, true, cx);
                }
                arrived = true;
                Some(waiting.draft)
            });
            let Composed { text, mut files } = draft.unwrap_or_default();
            let came = std::mem::take(&mut t.attached);
            arrived |= !came.is_empty();
            for problem in files.append(came) {
                cx.emit(PaneEvent::Error(problem));
            }
            t.attached = files;
            let end = text.len();
            t.composer.update(cx, |c, cx| {
                c.set_value(text, window, cx);
                c.set_selected_range(end..end, cx);
            });
            // Only for what came with it, since the hub has the rest, and another client may have changed it since this
            // one read it.
            if arrived {
                t.keep_typed(true, cx);
            }
        });
    }

    fn make_transcript(&mut self, session: Session, cx: &mut Context<Self>) {
        let (client, records, feed) = (self.client.clone(), self.shown.all().clone(), self.records.clone());
        let view = cx.new(|cx| {
            let mut transcript = Transcript::new(client, &records, &session, cx);
            transcript.set_thinking_open(crate::settings::get(cx).always_expand_thinking, cx);
            transcript.follow_records(feed);
            transcript.records_changed(&records, cx);
            transcript
        });
        Self::attach_transcript_hints(&view, cx);
        let chat = session.id.clone();
        let events = cx.subscribe(&view, move |this, transcript, event: &TranscriptEvent, cx| match event {
            TranscriptEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            TranscriptEvent::NotSent { message, prompt } => {
                this.give_back(Some(&chat), prompt.clone(), cx);
                cx.emit(PaneEvent::Error(message.clone()));
            }
            TranscriptEvent::OpenSubagent { subagent, title } => {
                this.open_subagent_tab(chat.clone(), subagent.clone(), title.clone(), cx);
            }
            TranscriptEvent::OpenProposal { proposal } => this.open_proposal_tab(chat.clone(), proposal, true, cx),
            TranscriptEvent::TurnEnded => this.turn_ended(cx),
            TranscriptEvent::Asking { left } => {
                // The question takes the composer's place.
                this.dictation_follows_its_composer(cx);
                if this.transcript().as_ref() == Some(&transcript) {
                    let left = left.clone();
                    this.with_window(cx, move |t, window, cx| t.follow_question(left.as_ref(), window, cx));
                }
                // What was chosen and typed in answer to it, before the app last stopped or in another client.
                this.take_answer(&chat, cx);
                cx.notify();
            }
            TranscriptEvent::Answering { at_once } => this.keep_answer(&chat, *at_once, cx),
            // Typing goes back to the message box, as it would have before the bar opened.
            TranscriptEvent::GoToWorkspace { workspace } => cx.emit(PaneEvent::GoToWorkspace(workspace.clone())),
            TranscriptEvent::FindClosed => this.with_window(cx, |t, window, cx| t.focus_showing(window, cx)),
        });
        self.chats.entry(session.id).or_default().transcript = Some((view, events));
        self.pass_on_agents(cx);
    }

    /// Asks first if it would stop an agent that's working.
    pub fn close_chat(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        let working = self.shown.session(&id).is_some_and(|s| s.status.is_active());
        if !working {
            self.finish_closing(id, cx);
            return;
        }
        let asked = Question {
            id: "close-chat",
            title: "Close this chat?".into(),
            says: "Its agent is still working, and closing the chat stops it. The history stays.".into(),
            keep: "Cancel",
            act: "Close chat",
            removes: false,
        };
        let this = cx.weak_entity();
        asking::ask(asked, window, cx, move |_, cx| {
            let _ = this.update(cx, |t, cx| t.finish_closing(id.clone(), cx));
        });
    }

    fn finish_closing(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let closing = self.client.start(move |client| client.session_close(&id));
        cx.spawn(async move |this, cx| {
            let closed = closing.recv().await;
            let _ = this.update(cx, |_, cx| match closed {
                // Deleted already, because nothing was sent in it, so closing it again has nothing left to do.
                Ok(_) | Err(ClientError::Api(ApiError { code: ErrorCode::NotFound, .. })) => {}
                Err(e) => cx.emit(PaneEvent::Error(client_core::refusals::couldnt("close the chat", &e))),
            });
        })
        .detach();
    }

    pub fn reopen_chat(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let reopening = self.client.start(move |client| client.session_reopen(&id));
        cx.spawn(async move |this, cx| {
            let reopened = reopening.recv().await;
            let _ = this.update(cx, |t, cx| match reopened {
                Ok(session) => t.show_chat_once_open(session.id, cx),
                Err(e) => cx.emit(PaneEvent::Error(client_core::refusals::couldnt("reopen the chat", &e))),
            });
        })
        .detach();
    }

    /// Asks what to call the chat, and renames it, starting from `current`.
    pub fn start_renaming_chat(&mut self, id: SessionId, current: &str, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        rename::open(Renaming::Chat, current, window, cx, move |title, cx| {
            let _ = this.update(cx, |t, cx| t.rename_chat(id.clone(), title, cx));
        });
    }

    fn rename_chat(&mut self, id: SessionId, title: String, cx: &mut Context<Self>) {
        let renaming = self.client.start(move |client| client.session_rename(&id, &title));
        cx.spawn(async move |this, cx| {
            let renamed = renaming.recv().await;
            if let Err(e) = renamed {
                let _ = this.update(cx, |_, cx| cx.emit(PaneEvent::Error(Renaming::Chat.failed(&e))));
            }
        })
        .detach();
    }

    /// Shows the chat `id` once what the pane shows of the records has it open, which it may already, since client-core
    /// takes in the record a reply carries before the reply comes.
    fn show_chat_once_open(&mut self, id: SessionId, cx: &mut Context<Self>) {
        self.showing_next = Some(Next::Chat(id));
        self.show_next(cx);
    }

    /// The workspace's sessions have changed since `earlier`, which the pane showed before.
    pub(super) fn set_sessions(&mut self, earlier: &InWorkspace, cx: &mut Context<Self>) {
        let was_open = earlier.sessions().iter().filter(|s| s.is_open()).map(|s| s.id.clone());
        let was_open: Vec<SessionId> = was_open.collect();
        // While the chat that shows is still the one that did, so a draft taken for the chat that shows next goes to
        // that chat's own, and moves into the composer with it.
        self.take_drafts_from_elsewhere(cx);
        self.keep_active_valid(&was_open, cx);
        self.keep_subagent_tab_valid(cx);
        self.keep_proposal_tab_valid(cx);
        self.keep_edit_valid(cx);
        self.showing_changed(cx);
        self.wind_cache_clock(cx);
        cx.notify();
    }

    /// Draws the pane again when the line above the composer next says something else of the shown chat's cache, as
    /// [`prompt_cache::composer_line_changes_in_ms`] says, then winds itself for the change after.
    fn wind_cache_clock(&mut self, cx: &mut Context<Self>) {
        let now_ms = look::now_ms(cx);
        let records = self.shown.all();
        let wait_ms =
            self.session().and_then(|session| prompt_cache::composer_line_changes_in_ms(records, session, now_ms));
        self.cache_clock = wait_ms.map(|wait_ms| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(wait_ms.unsigned_abs())).await;
                let _ = this.update(cx, |t, cx| {
                    t.wind_cache_clock(cx);
                    cx.notify();
                });
            })
        });
    }

    /// Moves off a chat that's been closed or deleted, here or anywhere else, to its neighbor among the chats that
    /// were open, and starts one if none are left.
    fn keep_active_valid(&mut self, was_open: &[SessionId], cx: &mut Context<Self>) {
        let open: Vec<SessionId> = self.open_sessions().map(|s| s.id.clone()).collect();
        // A chat that's gone takes everything the pane held for it: its transcript, and with it its subagents' and
        // proposals' records, and what was typed and attached in it, as the hub's draft does as it closes, rather than
        // going to the chat that shows instead.
        let gone: Vec<(SessionId, Chat)> = self.chats.extract_if(|id, _| !open.contains(id)).collect();
        for (id, chat) in gone {
            if let Some(draft) = chat.draft {
                self.let_go_of_files(&id, &draft.files);
            }
        }
        if self.active.as_ref().is_some_and(|id| open.contains(id)) {
            return;
        }
        if let Some(id) = self.active.clone() {
            let files = std::mem::take(&mut self.attached);
            self.let_go_of_files(&id, &files);
        }
        let next = match self.active.take() {
            Some(gone) => neighbor(was_open, &gone, &open),
            None => open.first().cloned(),
        };
        if let Some(id) = next {
            self.activate(id, cx);
        } else {
            self.show_draft(cx);
            self.show_chat(cx);
            self.ensure_chat(cx);
        }
    }
}
