//! Attaches shared dictation to the composer. The pane keeps only the chat being edited and the action to perform
//! after transcription, so recording mechanics are identical in prompts, question answers and a proposal's note.

use client_core::hints::{Around, Field, Workspace};
use gpui_kit::{App, Context, Window};
use protocol::model::SessionId;
use protocol::prompt::says_or_carries;

use super::{PaneEvent, Showing, WorkspacePane};

#[derive(Clone, Copy)]
pub(super) enum Then {
    Send,
    Queue,
}

impl WorkspacePane {
    pub(super) fn dictation_event(&mut self, event: &dictation_ui::Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            dictation_ui::Event::Started => self.talking(cx),
            dictation_ui::Event::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            dictation_ui::Event::Ended { submit } => {
                // In conversation mode, letting go of Space sends what was said.
                if self.send_said(window, cx) {
                    return cx.notify();
                }
                let then = self.after_dictation.ended(*submit);
                match then {
                    Some(Then::Send) => self.submit(window, cx),
                    Some(Then::Queue) => self.queue_typed(window, cx),
                    None => {}
                }
            }
        }
        cx.notify();
    }

    pub(super) fn dictation_follows_its_composer(&mut self, cx: &mut Context<Self>) {
        let question = self.transcript().is_some_and(|t| t.read(cx).question().is_some());
        let showing_chat = self.drawn() && self.showing() == Showing::Chat;
        if let Some(view) = self.proposals.view() {
            let showing = self.drawn() && self.showing() == Showing::Proposal;
            view.update(cx, |view, cx| view.set_in_sight(showing, cx));
        }
        if let Some(diff) = &self.diff {
            let showing = self.drawn()
                && self.showing() == Showing::File
                && matches!(self.file_tab_opened(), Some(super::Opened::Diff { .. }));
            diff.file.update(cx, |file, cx| file.set_dictation_enabled(showing, cx));
        }
        let drawn = self.active.clone().filter(|_| showing_chat && !question);
        if self.dictation_chat != drawn {
            self.end_dictation(cx);
            self.dictation_chat.clone_from(&drawn);
        }
        self.dictation.update(cx, |field, cx| field.set_enabled(drawn.is_some(), cx));
        for transcript in self.every_transcript() {
            if let Some(panel) = transcript.read(cx).question().cloned() {
                let showing = showing_chat && self.transcript().as_ref() == Some(transcript);
                panel.update(cx, |panel, cx| panel.set_dictation_enabled(showing, cx));
            }
        }
    }

    pub(super) fn end_dictation(&mut self, cx: &mut Context<Self>) {
        self.dictation.update(cx, dictation_ui::Field::end);
    }

    pub(super) fn once_dictated(&mut self, then: Then, cx: &mut Context<Self>) -> bool {
        let content = says_or_carries(&self.composer.read(cx).text().to_string(), self.attached.len());
        self.after_dictation.wait(&self.dictation, content, then, cx)
    }

    /// What's dictated into the composer is about, with the words to favour: what's typed, its files, then the
    /// workspace's and the chat's.
    pub(super) fn composer_about(&self, cx: &App) -> dictation_ui::About {
        let typed = self.composer.read(cx).text().to_string();
        let attached = self.attached.names();
        let open_file = self.diff.as_ref().and_then(|diff| diff.tab.as_ref()).map(|tab| tab.path.as_ref());
        let chat = self.transcript().map(|transcript| transcript.read(cx).log());
        let around = Around { workspace: Some(self.workspace_around(cx)), chat };
        let picked = Field::Composer { typed: &typed, attached: &attached, open_file, around }.picked();
        dictation_ui::About::new(self.composer_within(self.conversation_is_here(cx)), picked)
    }

    fn composer_within(&self, said_out_loud: bool) -> dictation_ui::Within {
        let workspace = Some(self.shown.workspace().id.to_string());
        dictation_ui::Within { workspace, chat: self.active.as_ref().map(ToString::to_string), said_out_loud }
    }

    /// Says that `text` was sent from the composer, for what's kept of dictation.
    pub(super) fn sent_from_composer(&self, text: String, said_out_loud: bool, cx: &mut Context<Self>) {
        let within = self.composer_within(said_out_loud);
        self.dictation.update(cx, |field, cx| field.sent(within, text, cx));
    }

    pub(super) fn attach_transcript_hints(
        transcript: &gpui_kit::Entity<transcript::Transcript>,
        cx: &mut Context<Self>,
    ) {
        let pane = cx.weak_entity();
        transcript.update(cx, |t, _| {
            t.set_dictation_hints(move |cx| {
                let workspace = pane.upgrade().map(|pane| pane.read(cx).workspace_around(cx));
                Around { workspace, chat: None }
            });
        });
    }

    /// What's around what's dictated about the chat `session` outside it: the workspace, then that chat.
    pub(super) fn chat_around<'a>(&'a self, session: &SessionId, cx: &'a App) -> Around<'a> {
        let transcript = self.chats.get(session).and_then(|chat| chat.transcript.as_ref());
        let chat = transcript.map(|(transcript, _)| transcript.read(cx).log());
        Around { workspace: Some(self.workspace_around(cx)), chat }
    }

    fn workspace_around<'a>(&'a self, cx: &'a App) -> Workspace<'a> {
        let listing = self.diff.as_ref().and_then(|diff| diff.review.read(cx).listing());
        let changed =
            listing.into_iter().flat_map(|listing| listing.rows.iter()).map(|file| file.path.as_ref()).collect();
        Workspace { shown: &self.shown, pull_request: self.pull_request().pull_request(), changed }
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn dictating(&self, cx: &gpui_kit::App) -> bool {
        self.dictation.read(cx).is_dictating()
    }
}
