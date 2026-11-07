//! What a client holds of one session: a window of its event log, never all of it.
//!
//! No IO and no UI. A view asks for rows by index, this says what to fetch, and whoever owns the
//! connection does the fetching. Rows are indexed by `seq - 1`, followed by prompts still waiting
//! to be recorded, so an index means the same thing however much is loaded.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use protocol::event::{EventBody, SessionEvent};
use protocol::model::{Seq, seq_index};
use protocol::tail::{StreamDelta, Tail};

pub const PAGE_EVENTS: u64 = 250;
/// Far more than fits on screen, and small enough that a 50 MB session stays cheap to hold.
pub const MAX_LOADED_EVENTS: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingPrompt {
    pub client_msg_id: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row<'a> {
    Event(&'a SessionEvent),
    /// In the log, but not loaded here. Worth asking for with [`SessionLog::pages_to_fetch`].
    Unloaded,
    /// Sent, and not yet recorded.
    Pending(&'a PendingPrompt),
}

#[derive(Debug, Default)]
pub struct SessionLog {
    last_seq: Seq,
    events: BTreeMap<Seq, SessionEvent>,
    pages: BTreeSet<u64>,
    in_flight: BTreeSet<u64>,
    pending: Vec<PendingPrompt>,
    tail: Tail,
}

fn page_of(seq: Seq) -> u64 {
    (seq.max(1) - 1) / PAGE_EVENTS
}

fn page_range(page: u64) -> Range<Seq> {
    page * PAGE_EVENTS + 1..(page + 1) * PAGE_EVENTS + 1
}

impl SessionLog {
    pub fn new(last_seq: Seq) -> Self {
        Self { last_seq, ..Default::default() }
    }

    pub fn row_count(&self) -> usize {
        seq_index(self.last_seq) + self.pending.len()
    }

    pub fn last_seq(&self) -> Seq {
        self.last_seq
    }

    pub fn row(&self, index: usize) -> Row<'_> {
        let seq = index as Seq + 1;
        if seq <= self.last_seq {
            return match self.events.get(&seq) {
                Some(event) => Row::Event(event),
                None => Row::Unloaded,
            };
        }
        match self.pending.get(index - seq_index(self.last_seq)) {
            Some(prompt) => Row::Pending(prompt),
            None => Row::Unloaded,
        }
    }

    pub fn tail(&self) -> &Tail {
        &self.tail
    }

    /// Open approvals among what's loaded. An approval blocks its agent, so an open one is always
    /// near the tail, which is always loaded.
    pub fn open_approvals(&self) -> Vec<&SessionEvent> {
        let mut open: Vec<&SessionEvent> = Vec::new();
        for event in self.events.values() {
            match &event.body {
                EventBody::ApprovalRequested { .. } => open.push(event),
                EventBody::ApprovalResolved { approval_id, .. } => open.retain(
                    |e| !matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id),
                ),
                EventBody::UserPrompt { .. }
                | EventBody::AgentStarted { .. }
                | EventBody::AgentInit { .. }
                | EventBody::AgentState { .. }
                | EventBody::Assistant { .. }
                | EventBody::AgentUser { .. }
                | EventBody::TurnEnded { .. }
                | EventBody::AgentExited { .. }
                | EventBody::Notice { .. }
                | EventBody::AgentMode { .. }
                | EventBody::AgentOther { .. }
                | EventBody::Unknown => {}
            }
        }
        open
    }

    /// Pages to fetch for a view looking at `focus`, nearest first, marking them as in flight.
    pub fn pages_to_fetch(&mut self, focus: usize) -> Vec<Range<Seq>> {
        if self.last_seq == 0 {
            return Vec::new();
        }
        let focus = focus.min(seq_index(self.last_seq) - 1) as Seq + 1;
        let center = page_of(focus);
        let last = page_of(self.last_seq);
        let mut wanted = vec![center];
        if center > 0 {
            wanted.push(center - 1);
        }
        if center < last {
            wanted.push(center + 1);
        }
        let mut out = Vec::new();
        for page in wanted {
            if !self.pages.contains(&page) && self.in_flight.insert(page) {
                out.push(page_range(page));
            }
        }
        out
    }

    /// Puts a fetched range in place. Returns the row indexes it filled, to re-measure.
    pub fn install_page(&mut self, start: Seq, events: Vec<SessionEvent>, focus: usize) -> Range<usize> {
        let page = page_of(start);
        self.in_flight.remove(&page);
        self.pages.insert(page);
        let range = page_range(page);
        for event in events {
            self.last_seq = self.last_seq.max(event.seq);
            self.events.insert(event.seq, event);
        }
        self.evict(focus);
        seq_index(range.start - 1)..seq_index((range.end - 1).min(self.last_seq))
    }

    /// A fetch failed, so the page can be asked for again.
    pub fn page_failed(&mut self, start: Seq) {
        self.in_flight.remove(&page_of(start));
    }

    /// Takes an event from a live subscription. Returns true if it added a row.
    pub fn apply_event(&mut self, event: SessionEvent) -> bool {
        if let EventBody::UserPrompt { client_msg_id: Some(id), .. } = &event.body {
            self.pending.retain(|prompt| prompt.client_msg_id != *id);
        }
        self.tail.on_event(&event.body);
        let added = event.seq > self.last_seq;
        self.last_seq = self.last_seq.max(event.seq);
        // Only the tail stays loaded from live events. Anything older is fetched when scrolled to.
        if page_of(event.seq) + 1 >= page_of(self.last_seq) {
            self.pages.insert(page_of(event.seq));
            self.events.insert(event.seq, event);
        }
        added
    }

    pub fn apply_tail(&mut self, tail: Tail) {
        self.tail = tail;
    }

    pub fn apply_delta(&mut self, delta: &StreamDelta) {
        self.tail.apply(delta);
    }

    /// Adds a prompt that's on its way, so it shows straight away. Recording it removes this.
    pub fn add_pending(&mut self, client_msg_id: String, text: String) {
        self.pending.push(PendingPrompt { client_msg_id, text });
    }

    /// The prompt couldn't be sent, so it no longer belongs in the transcript.
    pub fn remove_pending(&mut self, client_msg_id: &str) -> Option<PendingPrompt> {
        let at = self.pending.iter().position(|prompt| prompt.client_msg_id == client_msg_id)?;
        Some(self.pending.remove(at))
    }

    /// Drops whole pages furthest from `focus`, keeping the tail for live updates.
    fn evict(&mut self, focus: usize) {
        let center = page_of(focus as Seq + 1);
        let tail = page_of(self.last_seq);
        while self.events.len() > MAX_LOADED_EVENTS {
            let far = self
                .pages
                .iter()
                .copied()
                .filter(|page| page.abs_diff(center) > 1 && page.abs_diff(tail) > 1)
                .max_by_key(|page| page.abs_diff(center));
            let Some(far) = far else { return };
            self.pages.remove(&far);
            let range = page_range(far);
            let doomed: Vec<Seq> = self.events.range(range).map(|(seq, _)| *seq).collect();
            for seq in doomed {
                self.events.remove(&seq);
            }
        }
    }

    pub fn loaded_events(&self) -> usize {
        self.events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::event::{ApprovalOutcome, NoticeLevel};
    use protocol::tail::DeltaChange;
    use serde_json::Value;

    fn notice(seq: Seq) -> SessionEvent {
        SessionEvent {
            session_id: "s".into(),
            seq,
            ts_ms: 0,
            body: EventBody::Notice { level: NoticeLevel::Info, message: format!("{seq}") },
            raw: None,
        }
    }

    fn event(seq: Seq, body: EventBody) -> SessionEvent {
        SessionEvent { session_id: "s".into(), seq, ts_ms: 0, body, raw: None }
    }

    fn page(range: Range<Seq>) -> Vec<SessionEvent> {
        range.map(notice).collect()
    }

    #[test]
    fn rows_are_indexed_by_seq_whatever_is_loaded() {
        let mut log = SessionLog::new(1000);
        assert_eq!(log.row_count(), 1000);
        assert_eq!(log.row(0), Row::Unloaded);
        log.install_page(251, page(251..501), 300);
        assert!(matches!(log.row(250), Row::Event(e) if e.seq == 251));
        assert_eq!(log.row(249), Row::Unloaded);
        assert_eq!(log.row(500), Row::Unloaded);
    }

    #[test]
    fn it_asks_for_the_page_in_view_first_and_only_once() {
        let mut log = SessionLog::new(1000);
        let first = log.pages_to_fetch(300);
        assert_eq!(first, vec![251..501, 1..251, 501..751]);
        assert!(log.pages_to_fetch(300).is_empty(), "already in flight");

        log.page_failed(251);
        assert_eq!(log.pages_to_fetch(300), vec![251..501], "a failed page can be asked for again");

        assert!(SessionLog::new(0).pages_to_fetch(0).is_empty());
    }

    #[test]
    fn an_installed_page_reports_the_rows_it_filled_and_is_not_fetched_again() {
        let mut log = SessionLog::new(300);
        log.pages_to_fetch(0);
        assert_eq!(log.install_page(1, page(1..251), 0), 0..250);
        assert_eq!(log.install_page(251, page(251..301), 0), 250..300, "a short last page");
        assert!(log.pages_to_fetch(0).is_empty());
    }

    #[test]
    fn it_never_holds_a_whole_long_session() {
        let mut log = SessionLog::new(15_000);
        for start in (1..15_000).step_by(seq_index(PAGE_EVENTS)) {
            let focus = seq_index(start) - 1;
            log.install_page(start, page(start..start + PAGE_EVENTS), focus);
            assert!(log.loaded_events() <= MAX_LOADED_EVENTS + seq_index(PAGE_EVENTS));
        }
        // What's near the view and the tail stayed, and what was dropped can be fetched again.
        assert!(matches!(log.row(14_999), Row::Event(_)));
        assert_eq!(log.row(0), Row::Unloaded);
        assert_eq!(log.pages_to_fetch(0), vec![1..251, 251..501]);
    }

    #[test]
    fn a_prompt_shows_at_once_then_becomes_the_recorded_event() {
        let mut log = SessionLog::new(5);
        log.add_pending("m1".into(), "hello".into());
        assert_eq!(log.row_count(), 6);
        assert!(matches!(log.row(5), Row::Pending(p) if p.text == "hello"));

        log.apply_event(event(6, EventBody::UserPrompt { text: "hello".into(), client_msg_id: Some("m1".into()) }));
        assert_eq!(log.row_count(), 6, "it swapped in place");
        assert!(matches!(log.row(5), Row::Event(_)));
    }

    #[test]
    fn a_prompt_that_could_not_be_sent_can_be_taken_back() {
        let mut log = SessionLog::new(0);
        log.add_pending("m1".into(), "hello".into());
        assert_eq!(log.remove_pending("m1").unwrap().text, "hello");
        assert_eq!(log.row_count(), 0);
        assert_eq!(log.remove_pending("m1"), None);
    }

    #[test]
    fn live_events_grow_the_log_and_replays_do_not() {
        let mut log = SessionLog::new(2);
        assert!(log.apply_event(notice(3)));
        assert!(!log.apply_event(notice(3)));
        assert_eq!(log.row_count(), 3);
    }

    #[test]
    fn open_approvals_are_those_not_yet_answered() {
        let requested = |id: &str| EventBody::ApprovalRequested {
            approval_id: id.into(),
            tool_name: "Bash".into(),
            tool_use_id: None,
            input: Value::Null,
            description: None,
            suggestions: Value::Null,
        };
        let mut log = SessionLog::new(0);
        log.apply_event(event(1, requested("a")));
        log.apply_event(event(2, requested("b")));
        log.apply_event(event(
            3,
            EventBody::ApprovalResolved { approval_id: "a".into(), outcome: ApprovalOutcome::Allowed },
        ));
        let open = log.open_approvals();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].seq, 2);
    }

    #[test]
    fn the_streaming_tail_follows_deltas_and_clears_with_the_finished_message() {
        let mut log = SessionLog::new(0);
        log.apply_delta(&StreamDelta {
            message_id: "m".into(),
            parent_tool_use_id: None,
            change: DeltaChange::Text { index: 0, text: "hi".into() },
        });
        assert!(!log.tail().is_empty());
        log.apply_event(event(
            1,
            EventBody::Assistant {
                message_id: Some("m".into()),
                parent_tool_use_id: None,
                model: None,
                blocks: vec![],
            },
        ));
        assert!(log.tail().is_empty());
    }
}
