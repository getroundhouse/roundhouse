//! What the open chat's transcript has drawn, and waiting for it.

#![cfg(test)]

use gpui_kit::AppContext as _;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::SessionStatus;

use super::Ui;

impl Ui<'_> {
    pub(crate) fn events(&self) -> Vec<SessionEvent> {
        let Some(transcript) = self.transcript() else { return Vec::new() };
        self.cx.read_entity(&transcript, |t, _| (0..t.row_count()).filter_map(|row| t.event_at(row)).collect())
    }

    pub(crate) fn assistant_text(&self) -> String {
        let mut text = String::new();
        for event in self.events() {
            if let EventBody::Assistant { blocks, .. } = &event.body {
                for block in blocks {
                    if let ContentBlock::Text { text: t, .. } = block {
                        text.push_str(t);
                    }
                }
            }
        }
        text
    }

    pub(crate) fn session_status(&self) -> Option<SessionStatus> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().map(|s| s.status))
    }

    /// Waits until the chat is idle, its transcript has drawn the end of the turn that said the last of what it says,
    /// and that says what `text` wants. The chat's status comes on the hub's feed, apart from its events, so it can be
    /// idle before the last of the turn is drawn.
    pub(crate) fn wait_idle_with(&mut self, what: &str, mut text: impl FnMut(&str) -> bool) {
        self.wait(what, |ui| {
            ui.session_status() == Some(SessionStatus::Idle) && ui.turn_drawn_to_its_end() && text(&ui.assistant_text())
        });
    }

    /// Whether the transcript has drawn a turn's end after the last thing the agent said.
    fn turn_drawn_to_its_end(&self) -> bool {
        let events = self.events();
        let last = |is: fn(&EventBody) -> bool| events.iter().rposition(|event| is(&event.body));
        let ended = last(|body| matches!(body, EventBody::TurnEnded { .. }));
        ended >= last(|body| matches!(body, EventBody::Assistant { .. }))
    }
}
