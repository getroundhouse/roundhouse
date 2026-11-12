//! What's typed and attached in each chat and not sent, what's chosen and typed in answer to each chat's question, and
//! the comment being written on the diff. The hub keeps each, so it's there after the app restarts, and in every
//! client: the pane starts with what the hub has, reads back the files it kept, has it keep each change, and shows a
//! change made elsewhere, such as in another client. Quitting loses only what the hub hasn't had yet.

use std::collections::{HashMap, HashSet};

use client_core::{ClientError, FileToSend, Where};
use gpui_kit::{App, AppContext as _, Context, Entity};
use protocol::attachment::Attachment;
use protocol::model::{AttachmentId, Draft, NewDraft, Session, SessionId};
use protocol::prompt::says_or_carries;
use transcript::QuestionPanel;

use super::attaching::Composed;
use super::{PaneEvent, WorkspacePane};
use crate::attached::Attachments;

/// What the hub kept of each open chat's composer, for a pane to start with. The files come once they're read back.
pub(super) fn drafts_kept_in(sessions: &[Session]) -> HashMap<SessionId, Composed> {
    let kept = sessions.iter().filter(|s| s.closed_at_ms.is_none() && !s.draft.is_empty());
    kept.map(|s| (s.id.clone(), Composed { text: s.draft.text.clone(), files: Attachments::default() })).collect()
}

/// The files the hub kept with each open chat's draft, to read back.
pub(super) fn files_kept_in(sessions: &[Session]) -> HashMap<SessionId, Vec<Attachment>> {
    let kept = sessions.iter().filter(|s| s.closed_at_ms.is_none() && !s.draft.files.is_empty());
    kept.map(|s| (s.id.clone(), s.draft.files.clone())).collect()
}

/// Whether the hub keeps something typed or attached in one of these chats, for a workspace that has no pane yet.
pub(crate) fn drafted(sessions: &[Session]) -> bool {
    sessions.iter().any(|s| s.closed_at_ms.is_none() && says_or_carries(&s.draft.text, s.draft.files.len()))
}

/// Whether something's typed in one place, which the hub keeps once it has it, and whether it hasn't had all of it yet.
#[derive(Clone, Copy, Default)]
struct Typed {
    any: bool,
    unkept: bool,
}

impl Typed {
    fn lost(self) -> bool {
        self.any && self.unkept
    }
}

/// What a chat has that isn't sent.
#[derive(Clone, Copy, Default)]
struct Unsent {
    /// What's typed or attached in its composer.
    composed: Typed,
    /// What's typed in answer to its question.
    answering: Typed,
    /// What only this app has: a prompt still on its way to the hub, or one with no chat to keep it in yet.
    only_here: bool,
}

impl Unsent {
    fn any(self) -> bool {
        self.composed.any || self.answering.any || self.only_here
    }

    /// What stopping the app now would lose.
    fn lost(self) -> bool {
        self.only_here || self.composed.lost() || self.answering.lost()
    }
}

/// A file read back from the hub, or why it couldn't be.
type ReadBack = (Attachment, Result<Vec<u8>, ClientError>);

impl WorkspacePane {
    /// Has the hub keep what's typed and attached in the chat showing, once typing pauses, or at once for a change
    /// nobody's still typing, such as the composer emptied by sending.
    pub(super) fn keep_typed(&mut self, at_once: bool, cx: &App) {
        let Some(id) = self.active.clone() else { return };
        self.keep_chat(&id, at_once, cx);
    }

    /// Has the hub keep what's typed and attached in a chat at once, whether it's showing or not.
    pub(super) fn keep_draft(&mut self, id: &SessionId, cx: &App) {
        self.keep_chat(id, true, cx);
    }

    /// Hands the hub each file in the chat's composer it hasn't got, then the draft that names them. Files the hub kept
    /// that haven't been read back yet come first, where they'll be put back, so the draft goes on naming them.
    fn keep_chat(&mut self, id: &SessionId, at_once: bool, cx: &App) {
        let mut none = Attachments::default();
        let (text, files) = match self.drafts.get_mut(id) {
            _ if self.active.as_ref() == Some(id) => (self.composer.read(cx).text().to_string(), &mut self.attached),
            Some(draft) => (draft.text.clone(), &mut draft.files),
            None => (String::new(), &mut none),
        };
        for file in files.not_yet_given(id) {
            self.keeper.attach(id, file);
        }
        let coming_back = self.restoring.get(id).into_iter().flatten().map(|file| file.id.clone());
        let draft = NewDraft { text, files: coming_back.chain(files.ids()).collect() };
        if at_once {
            self.keeper.set(id, draft);
        } else {
            self.keeper.typed(id, draft);
        }
    }

    /// Has the hub let go of the files it was given for a chat that has closed. Closing takes the draft and the files
    /// it named, and this catches one that reached the hub after, such as one still being added as the chat closed.
    pub(super) fn let_go_of_files(&self, id: &SessionId, files: &Attachments) {
        for file in files.given_for(id) {
            self.keeper.detach(id, &file);
        }
    }

    /// Says that each open chat shows the draft on its record, as a pane does as it starts, so a newer record carrying
    /// the same draft isn't taken for one made elsewhere.
    pub(super) fn took_drafts(&self) {
        for session in self.open_sessions() {
            self.keeper.took(session);
        }
    }

    /// Shows in each open chat a draft made elsewhere since, such as in another client, in place of this app's. What's
    /// on its way to the hub from here goes on and replaces it, as [`client_core::Drafts::from_elsewhere`] says, so
    /// what's typed here is never undone as it's typed. Whatever comes in its place is the hub's already, so none of it
    /// is kept again.
    pub(crate) fn take_drafts_from_elsewhere(&mut self, cx: &mut Context<Self>) {
        let elsewhere: Vec<Session> = self.open_sessions().filter(|s| self.keeper.from_elsewhere(s)).cloned().collect();
        for session in elsewhere {
            self.keeper.took(&session);
            self.take_draft(&session.id, session.draft, cx);
        }
        if self.keeper.comment_from_elsewhere(&self.workspace) {
            self.take_writing(cx);
        }
        let asked = self.open_sessions().filter(|s| self.keeper.answer_from_elsewhere(s)).map(|s| s.id.clone());
        for chat in asked.collect::<Vec<_>>() {
            self.take_answer(&chat, cx);
        }
    }

    /// Opens the box for the comment being written as the hub has it, or takes it away for none, as the diff is made
    /// or as it changes elsewhere, and says this client shows it. The diff is made once, so there's nothing to take
    /// before it is.
    pub(super) fn take_writing(&self, cx: &mut Context<Self>) {
        let Some(diff) = &self.diff else { return };
        self.keeper.took_comment(&self.workspace);
        let (file, kept) = (diff.file.clone(), self.workspace.comment_draft.clone());
        self.with_window(cx, move |_, window, cx| file.update(cx, |f, cx| f.take_writing(kept.as_ref(), window, cx)));
    }

    /// The same, for what's chosen and typed in answer to the chat's question, as it's asked or as it changes
    /// elsewhere. One kept for a question that has gone means nothing.
    pub(super) fn take_answer(&self, chat: &SessionId, cx: &mut Context<Self>) {
        let (Some(panel), Some(session)) = (self.question(chat, cx), self.sessions.iter().find(|s| &s.id == chat))
        else {
            return;
        };
        self.keeper.took_answer(session);
        let question = panel.read(cx).approval_id().clone();
        let kept = session.answer_draft.clone().filter(|kept| kept.approval_id == question && kept.says_anything());
        self.with_window(cx, move |_, window, cx| panel.update(cx, |p, cx| p.take_so_far(kept.as_ref(), window, cx)));
    }

    /// The question the chat's agent is waiting on, as its transcript asks it.
    fn question(&self, chat: &SessionId, cx: &App) -> Option<Entity<QuestionPanel>> {
        self.transcripts.get(chat).and_then(|(transcript, _)| transcript.read(cx).question()).cloned()
    }

    /// Has the hub keep the comment being written on the diff, as the file tab asks.
    pub(super) fn keep_writing(&self, at_once: bool, cx: &App) {
        let writing = self.diff.as_ref().and_then(|diff| diff.file.read(cx).writing(cx));
        let id = &self.workspace.id;
        if at_once {
            self.keeper.set_comment(id, writing);
        } else {
            self.keeper.typed_comment(id, writing);
        }
    }

    /// Has the hub keep what's chosen and typed in answer to the chat's question, as its transcript asks.
    pub(super) fn keep_answer(&self, chat: &SessionId, at_once: bool, cx: &App) {
        let so_far = self.question(chat, cx).and_then(|panel| panel.read(cx).so_far(cx));
        if at_once {
            self.keeper.set_answer(chat, so_far);
        } else {
            self.keeper.typed_answer(chat, so_far);
        }
    }

    /// Puts a draft made elsewhere in its chat here: its text, the files here that it still names, and the rest once
    /// they're read back. A file it stopped naming is let go of by whoever took it out.
    fn take_draft(&mut self, id: &SessionId, draft: Draft, cx: &mut Context<Self>) {
        let named: HashSet<AttachmentId> = draft.files.iter().map(|file| file.id.clone()).collect();
        let files = self.files_for(Some(id));
        files.retain(|file| named.contains(file));
        let here: HashSet<AttachmentId> = files.ids().collect();
        let missing: Vec<Attachment> = draft.files.into_iter().filter(|file| !here.contains(&file.id)).collect();
        if self.active.as_ref() == Some(id) {
            let (chat, text) = (id.clone(), draft.text);
            self.with_window(cx, move |t, window, cx| {
                // Another chat may be showing by now, and it has its own. Setting the same text would still move the
                // cursor to the end and forget what undo had.
                if t.active.as_ref() != Some(&chat) || *t.composer.read(cx).text() == text {
                    return;
                }
                let end = text.len();
                t.composer.update(cx, |c, cx| {
                    c.set_value(text, window, cx);
                    c.set_selected_range(end..end, cx);
                });
            });
        } else {
            // Its files are its draft's, which `files_for` made if it had none.
            let here = self.drafts.entry(id.clone()).or_default();
            here.text = draft.text;
            if here.text.is_empty() && here.files.is_empty() {
                self.drafts.remove(id);
            }
        }
        if missing.is_empty() {
            self.restoring.remove(id);
        } else {
            self.restoring.insert(id.clone(), missing.clone());
            self.bring_back(id.clone(), missing, cx);
        }
        cx.notify();
    }

    /// Reads back the files the hub kept with each chat's draft that's waiting for them, off the main thread.
    pub(super) fn bring_back_files(&self, cx: &mut Context<Self>) {
        for (id, files) in &self.restoring {
            self.bring_back(id.clone(), files.clone(), cx);
        }
    }

    /// Reads back the files the hub kept with a chat's draft, off the main thread. A read isn't a command quitting
    /// waits for, as it can be a lot of bytes, and the hub keeps them anyway.
    fn bring_back(&self, chat: SessionId, files: Vec<Attachment>, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let reading = cx.background_spawn({
                let chat = chat.clone();
                async move {
                    let read = files.into_iter().map(|file| {
                        let data = client.attachment_read(&chat, &file.id);
                        (file, data)
                    });
                    read.collect::<Vec<ReadBack>>()
                }
            });
            let read = reading.await;
            let _ = this.update(cx, |t, cx| t.brought_back(&chat, read, cx));
        })
        .detach();
    }

    /// Puts a chat's kept files back in its composer, before any attached since, where every draft kept meanwhile has
    /// named them, so nothing needs keeping again, which could undo a change made elsewhere that hasn't arrived yet.
    /// Without a connection, they're read again on the next.
    fn brought_back(&mut self, chat: &SessionId, read: Vec<ReadBack>, cx: &mut Context<Self>) {
        // Not once they've come back, nor when the chat is waiting for others, such as a draft made elsewhere names.
        let read_ids = read.iter().map(|(file, _)| &file.id);
        let waiting = self.restoring.get(chat).is_some_and(|files| files.iter().map(|file| &file.id).eq(read_ids));
        let gone = |(_, data): &ReadBack| matches!(data, Err(ClientError::Io(_) | ClientError::Disconnected));
        if !waiting || read.iter().any(gone) {
            return;
        }
        self.restoring.remove(chat);
        let mut files = Vec::new();
        for (attachment, data) in read {
            match data {
                Ok(data) => files.push(FileToSend { attachment, data }),
                Err(e) => cx.emit(PaneEvent::Error(format!("couldn't bring back {}: {e}", attachment.name))),
            }
        }
        for problem in self.files_for(Some(chat)).bring_back(files, chat) {
            cx.emit(PaneEvent::Error(problem));
        }
        cx.notify();
    }

    /// What's here and not sent that stopping the app now would lose: how many chats have something only this app
    /// has, or that the hub hasn't had yet, and whether the comment being written on the diff has something the hub
    /// hasn't had.
    pub(crate) fn unsent(&self, cx: &App) -> (usize, usize) {
        let chats = self.unsent_chats(cx).filter(|unsent| unsent.lost()).count();
        let writing = self.diff.as_ref().and_then(|diff| diff.file.read(cx).writing(cx));
        let comment = writing.is_some_and(|writing| says_or_carries(&writing.text, 0));
        let unkept = self.keeper.unkept_in(&Where::Comment(self.workspace.id.clone()));
        (chats, usize::from(comment && unkept))
    }

    /// Whether a chat has something typed or attached that isn't sent, which the workspace's row shows with a pencil.
    pub(crate) fn has_unsent(&self, cx: &App) -> bool {
        self.unsent_chats(cx).any(Unsent::any)
    }

    /// For the chat showing, each other chat and a prompt waiting for the first chat to show, what it has that isn't
    /// sent. Each is only worked out as it's asked for, since the sidebar asks on every frame and wants only the first.
    fn unsent_chats<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = Unsent> + 'a {
        let answering = move |id: &SessionId| {
            let any = self.question(id, cx).is_some_and(|question| question.read(cx).has_typed(cx));
            Typed { any, unkept: self.keeper.unkept_in(&Where::Answer(id.clone())) }
        };
        let only_here = move |id: &SessionId| self.client.is_sending(id);
        let showing = std::iter::once_with(move || {
            // As `says_or_carries` says, without copying what's typed out of the composer.
            let text = self.composer.read(cx).text().chars().any(|c| !c.is_whitespace());
            let composed = text || !self.attached.is_empty();
            match &self.active {
                Some(id) => Unsent {
                    composed: Typed { any: composed, unkept: self.keeper.unkept(id) },
                    answering: answering(id),
                    only_here: only_here(id),
                },
                // While the workspace's first chat is being made.
                None => Unsent { only_here: composed, ..Unsent::default() },
            }
        });
        let drafted = move |id: &SessionId| Unsent {
            composed: Typed {
                // A chat whose files are on their way back has them on the hub.
                any: self.drafts.get(id).is_some_and(Composed::has_anything) || self.restoring.contains_key(id),
                unkept: self.keeper.unkept(id),
            },
            answering: answering(id),
            only_here: only_here(id),
        };
        let ids = self.drafts.keys().chain(self.transcripts.keys().filter(|id| !self.drafts.contains_key(*id)));
        let others = ids.filter(|id| self.active.as_ref() != Some(*id)).map(drafted);
        // Handed back before there was a chat to keep it in.
        let waiting = std::iter::once_with(|| Unsent {
            only_here: self.waiting.as_ref().is_some_and(|waiting| waiting.draft.has_anything()),
            ..Unsent::default()
        });
        showing.chain(others).chain(waiting)
    }
}
