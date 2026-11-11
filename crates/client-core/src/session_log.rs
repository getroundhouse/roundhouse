//! What a client holds of one session: a window of its event log, never all of it.
//!
//! No IO and no UI. A view asks for rows by index, this says what to fetch, and whoever owns the
//! connection does the fetching. Rows are indexed by `seq - 1`, followed by prompts still waiting
//! to be recorded, so an index means the same thing however much is loaded.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use protocol::attachment::Attachment;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Seq, seq_index};
use protocol::tail::{StreamDelta, Tail};

mod outline;
mod prompts;

pub use prompts::PromptState;

pub(crate) const PAGE_EVENTS: u64 = 250;
/// Far more than fits on screen, and small enough that a 50 MB session stays cheap to hold.
pub const MAX_LOADED_EVENTS: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingPrompt {
    pub client_msg_id: ClientMessageId,
    pub text: String,
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row<'a> {
    Event(&'a SessionEvent),
    /// In the log, but not loaded here. Worth asking for with [`SessionLog::pages_to_fetch`].
    Unloaded,
    /// Sent, and not yet recorded.
    Pending(&'a PendingPrompt),
}

/// What changed since a view last looked, gathered into one however many updates it took, so a view
/// that's slow to look sees the latest state rather than a backlog.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The rows whose content changed, as one range. The streaming tail is the row after the last.
    pub rows: Option<Range<usize>>,
    /// How many turns ended. Anything taken from the worktree may be out of date after one.
    pub turns_ended: usize,
}

#[derive(Debug, Default)]
pub struct SessionLog {
    last_seq: Seq,
    /// The newest event the subscription has delivered. It delivers every page after this, so those aren't
    /// fetched.
    received: Seq,
    events: BTreeMap<Seq, SessionEvent>,
    pages: BTreeSet<u64>,
    in_flight: BTreeSet<u64>,
    /// Pages whose fetch failed, which aren't asked for again until [`SessionLog::retry_failed`].
    failed: BTreeSet<u64>,
    /// The row a view last wanted, which decides what stays loaded. It starts at the last row, where a view opens a
    /// chat, so a log no view has asked of yet fetches nothing, since its end comes live.
    focus: usize,
    pending: Vec<PendingPrompt>,
    tail: Tail,
    changes: Changes,
    /// How many times events have been put in, live or by the page.
    events_added: u64,
    outline: outline::Outline,
}

fn page_of(seq: Seq) -> u64 {
    (seq.max(1) - 1) / PAGE_EVENTS
}

/// Where a subscription starts for a log that reaches `last_seq`: the start of the page before the newest,
/// so every page it delivers arrives whole, and an open approval, which is always near the end, comes too.
pub(crate) fn first_live_seq(last_seq: Seq) -> Seq {
    page_range(page_of(last_seq).saturating_sub(1)).start
}

fn page_range(page: u64) -> Range<Seq> {
    page * PAGE_EVENTS + 1..(page + 1) * PAGE_EVENTS + 1
}

impl SessionLog {
    /// A log that reaches `last_seq`, which a subscription fills from [`first_live_seq`] on.
    pub fn new(last_seq: Seq) -> Self {
        let focus = seq_index(last_seq).saturating_sub(1);
        Self { last_seq, received: first_live_seq(last_seq) - 1, focus, ..Default::default() }
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
                | EventBody::PromptTaken { .. }
                | EventBody::PromptDropped { .. }
                | EventBody::ToolCallDropped { .. }
                | EventBody::AgentStarted { .. }
                | EventBody::AgentInit { .. }
                | EventBody::AgentState { .. }
                | EventBody::AgentTasks { .. }
                | EventBody::AgentLimits { .. }
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

    /// The newest event the subscription has delivered, so a new one carries on after it.
    pub(crate) fn received(&self) -> Seq {
        self.received
    }

    /// Notes the row a view wants, which decides what to fetch and what stays loaded.
    pub(crate) fn focus_on(&mut self, row: usize) {
        self.focus = row;
    }

    /// Pages to fetch for the row a view wants, nearest first, marking them as in flight. A page the
    /// subscription has yet to deliver isn't one of them.
    pub(crate) fn pages_to_fetch(&mut self) -> Vec<Range<Seq>> {
        if self.last_seq == 0 {
            return Vec::new();
        }
        let focus = self.focus.min(seq_index(self.last_seq) - 1) as Seq + 1;
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
            let fetchable = page_range(page).start <= self.received && !self.failed.contains(&page);
            if fetchable && !self.pages.contains(&page) && self.in_flight.insert(page) {
                out.push(page_range(page));
            }
        }
        out
    }

    /// Puts a fetched range in place.
    pub(crate) fn install_page(&mut self, start: Seq, events: Vec<SessionEvent>) {
        let page = page_of(start);
        self.in_flight.remove(&page);
        self.pages.insert(page);
        let range = page_range(page);
        for event in events {
            self.last_seq = self.last_seq.max(event.seq);
            self.outline.saw(&event);
            self.events.insert(event.seq, event);
        }
        self.events_added += 1;
        self.evict();
        self.touched(seq_index(range.start - 1)..seq_index((range.end - 1).min(self.last_seq)));
    }

    /// A fetch failed, so the pages still on their way won't come. They aren't asked for again until
    /// [`SessionLog::retry_failed`], so a view looking at them doesn't keep asking.
    pub(crate) fn fetch_failed(&mut self) {
        self.failed.append(&mut self.in_flight);
    }

    /// Lets failed pages, and a failed list of prompts, be asked for again, such as on a new connection.
    pub(crate) fn retry_failed(&mut self) {
        self.failed.clear();
        self.outline.retry_failed();
    }

    /// Takes an event from a live subscription.
    pub fn apply_event(&mut self, event: SessionEvent) {
        if let EventBody::UserPrompt { client_msg_id: Some(id), .. } = &event.body {
            self.pending.retain(|prompt| prompt.client_msg_id != *id);
        }
        self.tail.on_event(event.run, &event.body);
        self.outline.saw(&event);
        self.received = self.received.max(event.seq);
        self.last_seq = self.last_seq.max(event.seq);
        if matches!(event.body, EventBody::TurnEnded { .. }) {
            self.changes.turns_ended += 1;
        }
        // Its own row, and everything after it, which a prompt it recorded may have moved, up to the tail.
        self.touched(seq_index(event.seq) - 1..self.row_count() + 1);
        // A prompt the agent dropped says so under it.
        if let EventBody::PromptDropped { prompt } = &event.body
            && *prompt > 0
        {
            self.touched(seq_index(*prompt) - 1..seq_index(*prompt));
        }
        // Only the tail stays loaded from live events. Anything older is fetched when scrolled to.
        if page_of(event.seq) + 1 >= page_of(self.last_seq) {
            self.pages.insert(page_of(event.seq));
            self.events.insert(event.seq, event);
            self.events_added += 1;
            self.evict();
        }
    }

    pub(crate) fn apply_tail(&mut self, tail: Tail) {
        self.tail = tail;
        self.touched(self.row_count()..self.row_count() + 1);
    }

    pub(crate) fn apply_delta(&mut self, delta: &StreamDelta) {
        self.tail.apply(delta);
        self.touched(self.row_count()..self.row_count() + 1);
    }

    /// Adds a prompt that's on its way, so it shows straight away. Recording it removes this.
    pub(crate) fn add_pending(&mut self, client_msg_id: ClientMessageId, text: String, attachments: Vec<Attachment>) {
        self.pending.push(PendingPrompt { client_msg_id, text, attachments });
        self.touched(seq_index(self.last_seq)..self.row_count() + 1);
    }

    /// What changed since this was last asked.
    pub(crate) fn take_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }

    fn touched(&mut self, rows: Range<usize>) {
        self.changes.rows = Some(match self.changes.rows.take() {
            Some(seen) => seen.start.min(rows.start)..seen.end.max(rows.end),
            None => rows,
        });
    }

    /// The prompt couldn't be sent, so it no longer belongs in the transcript.
    pub(crate) fn remove_pending(&mut self, client_msg_id: &ClientMessageId) -> Option<PendingPrompt> {
        let at = self.pending.iter().position(|prompt| prompt.client_msg_id == *client_msg_id)?;
        let removed = self.pending.remove(at);
        self.touched(seq_index(self.last_seq)..self.row_count() + 1);
        Some(removed)
    }

    /// Drops whole pages furthest from where a view is looking, keeping the tail for live updates.
    fn evict(&mut self) {
        let center = page_of(self.focus as Seq + 1);
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

    /// The rows that are loaded, as runs in order: each event held, and every prompt sent and not yet recorded.
    /// Every other row is [`Row::Unloaded`].
    pub fn loaded_rows(&self) -> Vec<Range<usize>> {
        let mut runs: Vec<Range<usize>> = Vec::new();
        let recorded = seq_index(self.last_seq);
        let events = self.events.keys().map(|seq| seq_index(*seq) - 1).filter(|row| *row < recorded);
        for row in events.chain(recorded..recorded + self.pending.len()) {
            match runs.last_mut() {
                Some(run) if run.end == row => run.end += 1,
                Some(_) | None => runs.push(row..row + 1),
            }
        }
        runs
    }

    pub fn loaded_events(&self) -> usize {
        self.events.len()
    }

    /// How many times events have been added to what's held. It changes whenever looking through the events again
    /// could find something that wasn't there before, so a view can keep what it found until then.
    pub fn events_added(&self) -> u64 {
        self.events_added
    }
}

#[cfg(test)]
mod laws;

#[cfg(test)]
mod tests;
