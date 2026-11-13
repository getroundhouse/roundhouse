//! What's typed and attached in each chat and not sent, what's chosen and typed in answer to each chat's question, and
//! the comment being written on the diff. The hub keeps each, so it's there after the app restarts, and in every
//! client: the pane starts with what the hub has, reads back the files it kept, has it keep each change, and shows a
//! change made elsewhere, such as in another client. Quitting loses only what the hub hasn't had yet.

use std::collections::HashSet;

use client_core::{CameBack, DraftChange, DraftKey, FilesFor, KeptDraft, ReadBack};
use gpui_kit::{App, Context, Entity};
use protocol::attachment::Attachment;
use protocol::model::{AttachmentId, CommentDraft, Draft, DraftToKeep, Session, SessionId};
use protocol::prompt::says_or_carries;
use transcript::QuestionPanel;

use super::attaching::Composed;
use super::{Chat, PaneEvent, WorkspacePane};
use crate::attached::{Attachments, read_back};

/// Whether the hub keeps something typed or attached in one of these chats, for a workspace that has no pane yet.
pub(crate) fn drafted(sessions: &[Session]) -> bool {
    sessions.iter().any(|s| s.is_open() && says_or_carries(&s.draft.text, s.draft.files.len()))
}

/// Whether something's typed in one draft, such as a chat's composer, which the hub keeps once it has it, and whether it
/// hasn't had all of it yet.
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
        let restoring = self.chats.get(id).and_then(|chat| chat.restoring.as_ref());
        let coming_back: Vec<AttachmentId> = restoring.into_iter().flatten().map(|file| file.id.clone()).collect();
        let mut none = Attachments::default();
        let (text, files) = match self.chats.get_mut(id).and_then(|chat| chat.draft.as_mut()) {
            _ if self.active.as_ref() == Some(id) => (self.composer.read(cx).text().to_string(), &mut self.attached),
            Some(draft) => (draft.text.clone(), &mut draft.files),
            None => (String::new(), &mut none),
        };
        let kept_for = FilesFor::Chat(id.clone());
        for file in files.not_yet_given(&kept_for) {
            self.keeper.attach(kept_for.clone(), file);
        }
        let files = coming_back.into_iter().chain(files.ids()).collect();
        let draft = DraftChange::Composer(id.clone(), DraftToKeep { text, files });
        if at_once {
            self.keeper.set(draft);
        } else {
            self.keeper.typed(draft);
        }
    }

    /// Has the hub let go of the files it was given for a chat that has closed. Closing takes the draft and the files
    /// it named, and this catches one that reached the hub after, such as one still being added as the chat closed.
    pub(super) fn let_go_of_files(&self, id: &SessionId, files: &Attachments) {
        let kept_for = FilesFor::Chat(id.clone());
        for file in files.given_for(&kept_for) {
            self.keeper.detach(kept_for.clone(), &file);
        }
    }

    /// Takes what the hub kept of each open chat's composer, from when this app or another last had it, as a pane does
    /// as it starts, before any chat shows, and says it shows each, so a newer record carrying the same draft isn't
    /// taken for one made elsewhere.
    pub(super) fn take_kept_drafts(&mut self, cx: &mut Context<Self>) {
        for session in self.open_sessions().cloned().collect::<Vec<Session>>() {
            self.keeper.took(KeptDraft::composer(&session));
            if !session.draft.is_empty() {
                self.take_draft(&session.id, session.draft, cx);
            }
        }
    }

    /// Shows in each open chat a draft made elsewhere since, such as in another client, in place of this app's. What's
    /// on its way to the hub from here goes on and replaces it, as [`client_core::Drafts::from_elsewhere`] says, so
    /// what's typed here is never undone as it's typed. Whatever comes in its place is the hub's already, so none of it
    /// is kept again.
    pub(crate) fn take_drafts_from_elsewhere(&mut self, cx: &mut Context<Self>) {
        let elsewhere = self.open_sessions().filter(|s| self.keeper.from_elsewhere(&KeptDraft::composer(s)));
        for session in elsewhere.cloned().collect::<Vec<Session>>() {
            self.keeper.took(KeptDraft::composer(&session));
            self.take_draft(&session.id, session.draft, cx);
        }
        if self.keeper.from_elsewhere(&KeptDraft::comment(self.workspace())) {
            self.take_writing(cx);
        }
        let asked = self.open_sessions().filter(|s| self.keeper.from_elsewhere(&KeptDraft::answer(s)));
        let asked = asked.map(|s| s.id.clone());
        for chat in asked.collect::<Vec<_>>() {
            self.take_answer(&chat, cx);
        }
    }

    /// Opens the box for the comment being written as the hub has it, or takes it away for none, as the diff is made
    /// or as it changes elsewhere, and says this client shows it. The diff is made once, so there's nothing to take
    /// before it is.
    pub(super) fn take_writing(&self, cx: &mut Context<Self>) {
        let Some(diff) = &self.diff else { return };
        self.keeper.took(KeptDraft::comment(self.workspace()));
        let (file, kept) = (diff.file.clone(), self.workspace().comment_draft.clone());
        self.with_window(cx, move |_, window, cx| file.update(cx, |f, cx| f.take_writing(kept.as_ref(), window, cx)));
    }

    /// The same, for what's chosen and typed in answer to the chat's question, as it's asked or as it changes
    /// elsewhere. One kept for a question that has gone means nothing.
    pub(super) fn take_answer(&self, chat: &SessionId, cx: &mut Context<Self>) {
        let (Some(panel), Some(session)) = (self.question(chat, cx), self.shown.session(chat)) else {
            return;
        };
        self.keeper.took(KeptDraft::answer(session));
        let question = panel.read(cx).approval_id().clone();
        let kept = session.answer_draft.clone().filter(|kept| kept.approval_id == question && kept.says_anything());
        self.with_window(cx, move |_, window, cx| panel.update(cx, |p, cx| p.take_so_far(kept.as_ref(), window, cx)));
    }

    /// The question the chat's agent is waiting on, as its transcript asks it.
    fn question(&self, chat: &SessionId, cx: &App) -> Option<Entity<QuestionPanel>> {
        let (transcript, _) = self.chats.get(chat)?.transcript.as_ref()?;
        transcript.read(cx).question().cloned()
    }

    /// Has the hub keep the comment being written on the diff, as the file tab asks.
    pub(super) fn keep_writing(&self, at_once: bool, cx: &App) {
        let writing = self.diff.as_ref().and_then(|diff| diff.file.read(cx).writing(cx));
        let writing = DraftChange::Comment(self.workspace().id.clone(), writing);
        if at_once {
            self.keeper.set(writing);
        } else {
            self.keeper.typed(writing);
        }
    }

    /// Saves the comment that was being written, once the hub has it as what's being written, so a keep of it from here
    /// can't arrive after the save that lets go of it there, and bring it back. Until the save arrives, the hub keeps
    /// it, so one that fails loses nothing.
    pub(super) fn save_writing(&self, saving: CommentDraft, cx: &mut Context<Self>) {
        let Some(diff) = &self.diff else { return };
        self.keeper.set(DraftChange::Comment(self.workspace().id.clone(), Some(saving.clone())));
        let kept = self.keeper.flushed();
        diff.review.update(cx, |review, cx| review.save_comment(saving, kept, cx));
    }

    /// Opens the box again with a comment that couldn't be saved.
    pub(super) fn give_back_writing(&self, saving: CommentDraft, cx: &mut Context<Self>) {
        let Some(diff) = &self.diff else { return };
        let file = diff.file.clone();
        self.with_window(cx, move |_, window, cx| file.update(cx, |f, cx| f.give_back_writing(saving, window, cx)));
    }

    /// Has the hub keep what's chosen and typed in answer to the chat's question, as its transcript asks.
    pub(super) fn keep_answer(&self, chat: &SessionId, at_once: bool, cx: &App) {
        let so_far = self.question(chat, cx).and_then(|panel| panel.read(cx).so_far(cx));
        let so_far = DraftChange::Answer(chat.clone(), so_far);
        if at_once {
            self.keeper.set(so_far);
        } else {
            self.keeper.typed(so_far);
        }
    }

    /// Puts a draft the hub keeps in its chat here, as the pane starts or as one made elsewhere arrives: its text, the
    /// files here that it still names, and the rest once they're read back. A file it stopped naming is let go of by
    /// whoever took it out.
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
            // Its files are its draft's, which `files_for` made if it had none. One whose files are still coming back
            // stays, so the chat still shows it has something.
            let chat = self.held(id);
            let here = chat.draft.get_or_insert_default();
            here.text = draft.text;
            if here.text.is_empty() && here.files.is_empty() && missing.is_empty() {
                chat.draft = None;
            }
        }
        if missing.is_empty() {
            self.held(id).restoring = None;
        } else {
            self.held(id).restoring = Some(missing.clone());
            self.bring_back(id.clone(), missing, cx);
        }
        cx.notify();
    }

    /// Reads back the files the hub kept with each chat's draft that's waiting for them, off the main thread.
    pub(super) fn bring_back_files(&self, cx: &mut Context<Self>) {
        for (id, chat) in &self.chats {
            if let Some(files) = &chat.restoring {
                self.bring_back(id.clone(), files.clone(), cx);
            }
        }
    }

    /// Reads back the files the hub kept with a chat's draft, off the main thread.
    fn bring_back(&self, chat: SessionId, files: Vec<Attachment>, cx: &mut Context<Self>) {
        let kept_for = FilesFor::Chat(chat.clone());
        read_back(
            self.client.clone(),
            kept_for,
            files,
            move |t: &mut Self, read, cx| t.brought_back(&chat, read, cx),
            cx,
        );
    }

    /// Puts a chat's kept files back in its composer, before any attached since, where every draft kept meanwhile has
    /// named them, so nothing needs keeping again, which could undo a change made elsewhere that hasn't arrived yet.
    /// Without a connection, they're read again on the next.
    fn brought_back(&mut self, chat: &SessionId, read: ReadBack, cx: &mut Context<Self>) {
        let waiting = self.chats.get(chat).and_then(|held| held.restoring.as_deref()).unwrap_or_default();
        let Some(CameBack { files, unread }) = read.came_back(waiting) else { return };
        self.held(chat).restoring = None;
        let mut problems: Vec<String> = unread.iter().map(ToString::to_string).collect();
        problems.extend(self.files_for(Some(chat)).bring_back(files, &FilesFor::Chat(chat.clone())));
        for problem in problems {
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
        let unkept = self.keeper.unkept(&DraftKey::Comment(self.workspace().id.clone()));
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
            Typed { any, unkept: self.keeper.unkept(&DraftKey::Answer(id.clone())) }
        };
        let only_here = move |id: &SessionId| self.client.is_sending(id);
        let showing = std::iter::once_with(move || {
            // As `says_or_carries` says, without copying what's typed out of the composer.
            let text = self.composer.read(cx).text().chars().any(|c| !c.is_whitespace());
            let composed = text || !self.attached.is_empty();
            match &self.active {
                Some(id) => {
                    // As for a chat not showing, one whose files are on their way back has them on the hub.
                    let restoring = self.chats.get(id).is_some_and(|chat| chat.restoring.is_some());
                    let unkept = self.keeper.unkept(&DraftKey::Composer(id.clone()));
                    Unsent {
                        composed: Typed { any: composed || restoring, unkept },
                        answering: answering(id),
                        only_here: only_here(id),
                    }
                }
                // While the workspace's first chat is being made.
                None => Unsent { only_here: composed, ..Unsent::default() },
            }
        });
        let drafted = move |(id, chat): (&SessionId, &Chat)| Unsent {
            composed: Typed {
                // A chat whose files are on their way back has them on the hub.
                any: chat.draft.as_ref().is_some_and(Composed::has_anything) || chat.restoring.is_some(),
                unkept: self.keeper.unkept(&DraftKey::Composer(id.clone())),
            },
            answering: answering(id),
            only_here: only_here(id),
        };
        // Each chat with a draft or a transcript, other than the one showing.
        let others = self.chats.iter().filter(|(id, chat)| {
            (chat.draft.is_some() || chat.transcript.is_some()) && self.active.as_ref() != Some(*id)
        });
        let others = others.map(drafted);
        // Handed back before there was a chat to keep it in.
        let waiting = std::iter::once_with(|| Unsent {
            only_here: self.waiting.as_ref().is_some_and(|waiting| waiting.draft.has_anything()),
            ..Unsent::default()
        });
        showing.chain(others).chain(waiting)
    }
}
