//! Files waiting in a chat's composer to go with its next prompt, kept with the chat while another is showing, and a
//! prompt given back because it couldn't be sent. `crate::attached` reads, checks and draws the files.

use std::path::PathBuf;

use client_core::FileToSend;
use gpui_kit::{AnyElement, ClipboardItem, Context, IntoElement as _, Styled as _};
use protocol::model::{AttachmentId, SessionId};
use protocol::prompt::says_or_carries;

use super::{PaneEvent, WorkspacePane};
use crate::attached::{self, Attachments, joined};

/// What a chat's composer holds while another chat is showing.
#[derive(Default)]
pub(crate) struct Draft {
    pub(crate) text: String,
    pub(crate) files: Attachments,
}

impl Draft {
    pub(crate) fn has_anything(&self) -> bool {
        says_or_carries(&self.text, self.files.len())
    }
}

/// A prompt handed back before the workspace had a chat to show, for the first that shows.
#[derive(Default)]
pub(crate) struct Waiting {
    pub(crate) draft: Draft,
    /// Whether it was to be planned, so that chat is asked into plan mode.
    pub(crate) to_plan: bool,
}

impl WorkspacePane {
    /// Takes a paste that holds images or copied files, and leaves text to the composer. Returns whether it took it.
    pub fn paste(&self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let Some(reading) = attached::pasted(item) else { return false };
        // The chat showing when the files came, even if another is showing by the time they're read.
        let chat = self.active.clone();
        attached::read_into(
            reading,
            cx,
            move |t| t.files_for(chat.as_ref()),
            |_, problem, cx| {
                cx.emit(PaneEvent::Error(problem));
            },
        );
        true
    }

    /// Files dropped on the composer, or chosen in the file picker.
    pub(crate) fn attach_paths(&self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let chat = self.active.clone();
        attached::read_paths_into(
            paths,
            cx,
            move |t| t.files_for(chat.as_ref()),
            |_, problem, cx| {
                cx.emit(PaneEvent::Error(problem));
            },
        );
    }

    /// The names of what's attached in the chat showing, in order, for a test to see what a paste or drop did.
    #[cfg(feature = "testing")]
    pub fn attached_names(&self) -> Vec<String> {
        self.attached.names()
    }

    pub(crate) fn remove_attached(&mut self, id: &AttachmentId, cx: &mut Context<Self>) {
        self.attached.remove(id);
        cx.notify();
    }

    /// Puts back a prompt that couldn't be sent, in `chat`, so nothing typed or attached is lost. It goes before
    /// anything typed or attached since. With no chat, it goes to the one showing, or if none is yet, such as while the
    /// hub makes a new workspace's first, to the first that shows.
    pub(crate) fn give_back(
        &mut self,
        chat: Option<&SessionId>,
        text: String,
        files: Vec<FileToSend>,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = chat.or(self.active.as_ref()).cloned() else {
            let waiting = &mut self.waiting.get_or_insert_default().draft;
            let problems = waiting.files.put_back(files);
            if !text.is_empty() {
                waiting.text = joined(text, &waiting.text);
            }
            for problem in problems {
                cx.emit(PaneEvent::Error(problem));
            }
            return;
        };
        for problem in self.files_for(Some(&chat)).put_back(files) {
            cx.emit(PaneEvent::Error(problem));
        }
        if !text.is_empty() {
            if self.active.as_ref() == Some(&chat) {
                // Setting the composer needs the window.
                self.with_window(cx, move |t, window, cx| {
                    let text = joined(text, &t.composer.read(cx).text().to_string());
                    t.composer.update(cx, |c, cx| c.set_value(text, window, cx));
                });
            } else {
                let draft = self.drafts.entry(chat).or_default();
                draft.text = joined(text, &draft.text);
            }
        }
        cx.notify();
    }

    /// Asks for plan mode in `chat`, for a prompt handed back that was to be planned, so it isn't then sent with full
    /// access. With no chat, it's asked for in the one [`Self::give_back`] gave the prompt to.
    pub(crate) fn plan_given_back(&mut self, chat: Option<&SessionId>, cx: &mut Context<Self>) {
        match chat.or(self.active.as_ref()).cloned() {
            Some(chat) => self.want_plan_mode(chat, true, cx),
            None => self.waiting.get_or_insert_default().to_plan = true,
        }
    }

    /// The files waiting in `chat`'s composer: the composer's own while it's showing, or else its draft's.
    fn files_for(&mut self, chat: Option<&SessionId>) -> &mut Attachments {
        match chat {
            Some(chat) if self.active.as_ref() != Some(chat) => &mut self.drafts.entry(chat.clone()).or_default().files,
            _ => &mut self.attached,
        }
    }

    /// The tiles above what's typed.
    pub(crate) fn attached_tiles(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tiles = self.attached.tiles("composer-attachments", cx, Self::remove_attached)?;
        Some(tiles.px_3().pt_3().into_any_element())
    }
}
