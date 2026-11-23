//! What the open chat's transcript has drawn, and waiting for it.

#![cfg(test)]

use gpui_kit::AppContext as _;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Seq, SessionStatus};

use super::Ui;

impl Ui<'_> {
    /// The events the open chat's transcript draws its rows from, which can be behind its session's log by what the
    /// transcript hasn't caught up with yet.
    pub(crate) fn events(&self) -> Vec<SessionEvent> {
        let Some(transcript) = self.transcript() else { return Vec::new() };
        self.cx.read_entity(&transcript, |t, _| (0..t.row_count()).filter_map(|row| t.drawn_event_at(row)).collect())
    }

    /// Waits for the open chat's list to settle where it is: every row it has measured and every long text in it
    /// parsed, with no frame asked for. A page changes the session's height in three goes: as its rows in the window
    /// are measured, over the frames after as the rest of them are, a few to a frame, and as each long text in it is
    /// parsed on another thread and its row measured again. So where the list is, read before then, isn't where it
    /// stays.
    #[track_caller]
    pub(crate) fn wait_for_the_chat_to_settle(&mut self) {
        let Some(transcript) = self.transcript() else { return };
        let settled = |ui: &mut Self| {
            ui.settle(super::motion::A_FRAME);
            ui.cx.read_entity(&transcript, |t, _| t.list().rows_to_measure() == 0 && t.long_texts_parsed())
        };
        let loading = |ui: &mut Self| ui.cx.read_entity(&transcript, |t, _| t.loading());
        self.wait_saying("the chat to settle", settled, loading);
    }

    /// The row the open chat's log has `event` in, whether it's loaded or not, as its position there says. It has to be
    /// one of the events the chat follows, which by place are the agent's own.
    pub(crate) fn row_of(&self, event: &SessionEvent) -> usize {
        let transcript = self.transcript().unwrap();
        let row = self.cx.read_entity(&transcript, |t, _| t.event_row(event));
        row.unwrap_or_else(|| panic!("the chat doesn't follow {event:?}"))
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

    /// Has the chat keep to its end again, as someone does who scrolls back down to it. A line that's clicked open
    /// stays where it is in the window, so what it opened to can run on past the window's bottom.
    pub(crate) fn keep_to_the_end(&mut self) {
        let transcript = self.transcript().unwrap();
        self.cx.update_entity(&transcript, |t, cx| {
            t.list().go_to_latest();
            cx.notify();
        });
    }

    pub(crate) fn session_status(&self) -> Option<SessionStatus> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().map(|s| s.status))
    }

    /// Waits until the chat is idle, its transcript has drawn the end of its newest turn, as [`Ui::turn_drawn_to_its_end`]
    /// says, and that says what `text` wants. The chat's status comes on the hub's feed, apart from its events, so it can
    /// be idle before the last of the turn is drawn.
    pub(crate) fn wait_idle_with(&mut self, what: &str, mut text: impl FnMut(&str) -> bool) {
        self.wait(what, |ui| {
            ui.session_status() == Some(SessionStatus::Idle) && ui.turn_drawn_to_its_end() && text(&ui.assistant_text())
        });
    }

    /// Whether the transcript has drawn the end of the newest turn the chat's record names, with nothing the agent said
    /// after it. The record and the events come apart, so an earlier turn's end can be drawn while the record has moved
    /// on, and the log hasn't yet taken the newest turn's first words. So it's the end the record names, by its `seq`,
    /// as the transcript itself compares against.
    pub(crate) fn turn_drawn_to_its_end(&self) -> bool {
        let Some(newest) = self.reply_seq() else { return false };
        let events = self.events();
        let Some(ended) = events.iter().position(|event| event.seq == newest) else { return false };
        let said = events.iter().rposition(|event| matches!(event.body, EventBody::Assistant { .. }));
        said.is_none_or(|said| said < ended)
    }

    /// The event that ended the newest turn, as the open chat's record says, which moves once another turn has ended.
    pub(crate) fn reply_seq(&self) -> Option<Seq> {
        let pane = self.pane()?;
        self.cx.read_entity(&pane, |p, _| p.session().map(|s| s.reply_seq))
    }
}
