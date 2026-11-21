//! What's typed in the composer and not sent, which the hub keeps for each chat, so it's there in every client and
//! after the app restarts, as the desktop's is. What's typed here is handed over as it changes, and a draft the
//! chat's record says was made elsewhere, such as on the desktop, takes its place, as client-core's `Drafts` says.
//! Files the desktop attached go on being named, since the phone can't show them, and leaving them out would lose
//! them.

use client_core::{DraftChange, Drafts, KeptDraft};
use gpui_kit::Context;
use protocol::model::{DraftToKeep, Session};

use super::view::ChatView;

/// What has the hub keep the composer's draft.
pub(super) struct Kept {
    drafts: Drafts,
    /// What the hub has of the composer, or has been handed, so a draft put in the composer from the hub isn't handed
    /// back to it as typed.
    text: String,
}

impl Kept {
    pub(super) fn new(drafts: Drafts) -> Self {
        Self { drafts, text: String::new() }
    }
}

impl ChatView {
    /// Has the hub keep what's typed, once typing pauses, unless it's what the hub has already. `at_once` keeps it now,
    /// for a change nobody's still typing, such as the composer emptied by sending.
    pub(super) fn keep_typed(&mut self, at_once: bool, cx: &mut Context<Self>) {
        let Some(shown) = &self.shown else { return };
        let text = self.composer.read(cx).value().to_string();
        if text == self.kept.text {
            return;
        }
        self.kept.text.clone_from(&text);
        let files = shown.session.draft.files.iter().map(|file| file.id.clone()).collect();
        let draft = DraftToKeep { text, files };
        let change = DraftChange::Composer(shown.session.id.clone(), draft);
        if at_once {
            self.kept.drafts.set(change);
        } else {
            self.kept.drafts.typed(change);
        }
    }

    /// Puts what `session`'s composer says in it: what this client handed over for it, while that hasn't reached the
    /// hub, and otherwise what the hub kept, as `client_core::Drafts::take_composer` says.
    pub(super) fn take_draft(&mut self, session: &Session) {
        let text = self.kept.drafts.take_composer(session);
        self.kept.text.clone_from(&text);
        self.put_back = Some(text);
    }

    /// Shows a draft of the chat made elsewhere since, in place of what's typed here. None is shown while what's typed
    /// here is on its way to the hub, so it's never undone as it's typed, and the hub keeps whichever of the two was
    /// typed last, as `client_core::Drafts::from_elsewhere` says.
    pub(super) fn take_draft_from_elsewhere(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.shown.as_ref().map(|shown| shown.session.clone()) else { return };
        if self.kept.drafts.from_elsewhere(&KeptDraft::composer(&session)) {
            self.take_draft(&session);
            cx.notify();
        }
    }
}
