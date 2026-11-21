//! Attaches shared dictation to the composer. The pane keeps only the chat being edited and the action to perform
//! after transcription, so recording mechanics are identical in prompts, question answers and a proposal's note.

use client_core::hints::HintCollector;
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
            dictation_ui::Event::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            dictation_ui::Event::Ended { submit } => {
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
            let showing =
                self.drawn() && self.showing() == Showing::File && self.file_tab_opened() == Some(super::Opened::Diff);
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

    pub(super) fn composer_hints(&self, cx: &App) -> dictation_ui::Hints {
        let mut collector = HintCollector::new();
        collector.add_prose(&self.composer.read(cx).text().to_string());
        for name in self.attached.names() {
            collector.add_path(&name);
        }
        if let Some(tab) = self.diff.as_ref().and_then(|diff| diff.tab.as_ref()) {
            collector.add_path(&tab.path);
        }
        self.collect_workspace_hints(&mut collector, cx);
        if let Some(transcript) = self.transcript() {
            collector.add_session_log(transcript.read(cx).log());
        }
        dictation_ui::Hints::new(collector.finish())
    }

    pub(super) fn attach_transcript_hints(
        transcript: &gpui_kit::Entity<transcript::Transcript>,
        cx: &mut Context<Self>,
    ) {
        let pane = cx.weak_entity();
        transcript.update(cx, |t, _| {
            t.set_dictation_hints(move |collector, cx| {
                if let Some(pane) = pane.upgrade() {
                    pane.read(cx).collect_workspace_hints(collector, cx);
                }
            });
        });
    }

    /// The workspace's hint words, then those of the chat `session`, for what's dictated about it outside it.
    pub(super) fn collect_chat_hints(&self, session: &SessionId, collector: &mut HintCollector, cx: &App) {
        self.collect_workspace_hints(collector, cx);
        if let Some((transcript, _)) = self.chats.get(session).and_then(|chat| chat.transcript.as_ref()) {
            collector.add_session_log(transcript.read(cx).log());
        }
    }

    fn collect_workspace_hints(&self, collector: &mut HintCollector, cx: &App) {
        collector.add_workspace(&self.shown);
        if let Some(pull_request) = self.pull_request.pull_request() {
            collector.add_pull_request(pull_request);
        }
        if let Some(files) = self.diff.as_ref().and_then(|diff| diff.review.read(cx).files.as_ref()) {
            for patch in files {
                collector.add_path(patch.path());
            }
        }
    }

    #[cfg(feature = "testing")]
    pub fn dictating(&self, cx: &gpui_kit::App) -> bool {
        self.dictation.read(cx).is_dictating()
    }
}
