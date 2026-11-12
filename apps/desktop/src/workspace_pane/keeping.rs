//! What's typed and attached in each chat and not sent. The hub keeps each chat's, so it's there after the app
//! restarts: the pane starts with what the hub has, reads back the files it kept, and has it keep each change. An
//! answer being typed to a question stays only here, so it's what quitting loses, with anything the hub hasn't had
//! yet.

use std::collections::HashMap;

use client_core::{ClientError, FileToSend};
use gpui_kit::{App, AppContext as _, Context};
use protocol::attachment::Attachment;
use protocol::model::{NewDraft, Session, SessionId};
use protocol::prompt::says_or_carries;

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

/// What a chat has that isn't sent.
#[derive(Clone, Copy)]
struct Unsent {
    /// Something typed or attached in its composer, which the hub keeps once it has it.
    composed: bool,
    /// The hub hasn't had all of what's in the composer yet.
    unkept: bool,
    /// What only this app has: an answer being typed to its question, a prompt still on its way to the hub, or one
    /// with no chat to keep it in yet.
    only_here: bool,
}

impl Unsent {
    fn any(self) -> bool {
        self.composed || self.only_here
    }

    /// What stopping the app now would lose.
    fn lost(self) -> bool {
        self.only_here || (self.composed && self.unkept)
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

    /// Reads back the files the hub kept with each chat's draft that's waiting for them, off the main thread. A read
    /// isn't a command quitting waits for, as it can be a lot of bytes, and the hub keeps them anyway.
    pub(super) fn bring_back_files(&self, cx: &mut Context<Self>) {
        for (id, files) in &self.restoring {
            let (client, chat, files) = (self.client.clone(), id.clone(), files.clone());
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
    }

    /// Puts a chat's kept files back in its composer, before any attached since, then has the hub keep what the chat
    /// has now. Without a connection, they're read again on the next.
    fn brought_back(&mut self, chat: &SessionId, read: Vec<ReadBack>, cx: &mut Context<Self>) {
        let gone = |(_, data): &ReadBack| matches!(data, Err(ClientError::Io(_) | ClientError::Disconnected));
        if read.iter().any(gone) || self.restoring.remove(chat).is_none() {
            return;
        }
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
        self.keep_draft(chat, cx);
        cx.notify();
    }

    /// What's here and not sent that stopping the app now would lose: how many chats have something only this app
    /// has, or that the hub hasn't had yet, and how many comments on the diff are being written.
    pub(crate) fn unsent(&self, cx: &App) -> (usize, usize) {
        let chats = self.unsent_chats(cx).filter(|unsent| unsent.lost()).count();
        let draft = self.diff.as_ref().and_then(|diff| diff.file.read(cx).draft.as_ref());
        let comment = draft.is_some_and(|draft| says_or_carries(&draft.input.read(cx).text().to_string(), 0));
        (chats, usize::from(comment))
    }

    /// Whether a chat has something typed or attached that isn't sent, which the workspace's row shows with a pencil.
    pub(crate) fn has_unsent(&self, cx: &App) -> bool {
        self.unsent_chats(cx).any(Unsent::any)
    }

    /// For the chat showing, each other chat and a prompt waiting for the first chat to show, what it has that isn't
    /// sent. Each is only worked out as it's asked for, since the sidebar asks on every frame and wants only the first.
    fn unsent_chats<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = Unsent> + 'a {
        let only_here = move |id: &SessionId| {
            let question = self.transcripts.get(id).and_then(|(transcript, _)| transcript.read(cx).question());
            question.is_some_and(|question| question.read(cx).has_typed(cx)) || self.client.is_sending(id)
        };
        let showing = std::iter::once_with(move || {
            // As `says_or_carries` says, without copying what's typed out of the composer.
            let text = self.composer.read(cx).text().chars().any(|c| !c.is_whitespace());
            let composed = text || !self.attached.is_empty();
            match &self.active {
                Some(id) => Unsent { composed, unkept: self.keeper.unkept(id), only_here: only_here(id) },
                // While the workspace's first chat is being made.
                None => Unsent { composed: false, unkept: false, only_here: composed },
            }
        });
        let drafted = move |id: &SessionId| Unsent {
            // A chat whose files are on their way back has them on the hub.
            composed: self.drafts.get(id).is_some_and(Composed::has_anything) || self.restoring.contains_key(id),
            unkept: self.keeper.unkept(id),
            only_here: only_here(id),
        };
        let ids = self.drafts.keys().chain(self.transcripts.keys().filter(|id| !self.drafts.contains_key(*id)));
        let others = ids.filter(|id| self.active.as_ref() != Some(*id)).map(drafted);
        // Handed back before there was a chat to keep it in.
        let waiting = std::iter::once_with(|| Unsent {
            composed: false,
            unkept: false,
            only_here: self.waiting.as_ref().is_some_and(|waiting| waiting.draft.has_anything()),
        });
        showing.chain(others).chain(waiting)
    }
}
