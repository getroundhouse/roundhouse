//! Attaches shared dictation to the composer. The pane keeps only the chat being edited and the action to perform
//! after transcription, so recording mechanics are identical in prompts and question answers.

use gpui_kit::{Context, Window};
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
                let then = self.after_dictation.take().filter(|_| *submit);
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
        self.after_dictation = None;
        self.dictation.update(cx, dictation_ui::Field::end);
    }

    pub(super) fn once_dictated(&mut self, then: Then, cx: &mut Context<Self>) -> bool {
        let content = says_or_carries(&self.composer.read(cx).text().to_string(), self.attached.len());
        let wait = self.dictation.update(cx, |field, cx| field.finish_before_submit(content, cx));
        if wait {
            self.after_dictation = Some(then);
        }
        wait
    }

    #[cfg(feature = "testing")]
    pub fn dictating(&self, cx: &gpui_kit::App) -> bool {
        self.dictation.read(cx).is_dictating()
    }

    #[cfg(feature = "testing")]
    pub fn hear(&self, audio: &[f32], cx: &gpui_kit::App) {
        self.dictation.read(cx).hear(audio);
    }
}
