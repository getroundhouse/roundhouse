//! What a client holds of one session: a window of its event log, or of the events from one agent or subagent in
//! it, never all of it.
//!
//! No IO and no UI. A view asks for rows by index, this says what to fetch, and whoever owns the
//! connection does the fetching. Rows are indexed by each event's position less one, so an index means the same thing
//! however much is loaded. An event's position is its `seq` in the whole log, and its place in the events from one
//! agent or subagent. The tail comes after the rows, with the prompts the agent hasn't read yet, recorded or still on
//! their way, as [`PromptAt`] says.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, QueuedPrompt, Seq, SettledAt, seq_index};
use protocol::tail::{StreamDelta, Tail};

mod approvals;
mod outline;
mod pages;
mod prompts;
mod restart;

pub use approvals::RecordedApprovals;
use pages::first_live;
pub use prompts::{PromptAt, PromptState, Settled};

/// Far more than fits on screen, and small enough that a 50 MB session stays cheap to hold.
pub const MAX_LOADED_EVENTS: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row<'a> {
    Event(&'a SessionEvent),
    /// In the log, but not loaded here. Worth asking for with [`SessionLog::pages_to_fetch`].
    Unloaded,
}

/// What changed since a view last looked, gathered into one however many updates it took, so a view
/// that's slow to look sees the latest state rather than a backlog.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The rows whose content changed, as one range. The streaming tail is the row after the last.
    pub rows: Option<Range<usize>>,
    /// How many turns ended. Anything taken from the worktree may be out of date after one.
    pub turns_ended: usize,
    /// A request waiting on an answer was fetched on its own, so what waits is to be asked again, though no row
    /// changed.
    pub requests_fetched: bool,
}

/// What a log's rows follow, and so what an event's position is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum By {
    /// Every event in the session, by `seq`.
    #[default]
    Seq,
    /// The events from one agent or subagent, by their place among them.
    Place,
}

#[derive(Debug, Default)]
pub struct SessionLog {
    by: By,
    /// The position of the newest event.
    last: u64,
    /// The newest event the subscription has delivered. It delivers every page after this, so those aren't
    /// fetched.
    received: u64,
    /// What's loaded, by position.
    events: BTreeMap<u64, SessionEvent>,
    /// The position of each event loaded, by its `seq`, when that isn't its position.
    positions: HashMap<Seq, u64>,
    pages: BTreeSet<u64>,
    in_flight: BTreeSet<u64>,
    /// Pages whose fetch failed, which aren't asked for again until [`SessionLog::retry_failed`].
    failed: BTreeSet<u64>,
    /// The row a view last wanted, which decides what stays loaded. It starts at the last row, where a view opens a
    /// chat, so a log no view has asked of yet fetches nothing, since its end comes live.
    focus: usize,
    /// Sent, and not yet recorded. Each is held as the hub holds a queued prompt, under the ID it'll be recorded under.
    pending: Vec<QueuedPrompt>,
    tail: Tail,
    changes: Changes,
    /// How many times events have been put in, live or by the page.
    events_added: u64,
    outline: outline::Outline,
    settled: Settled,
    /// The requests still waiting on an answer that no page loaded here holds.
    apart: approvals::Apart,
}

impl SessionLog {
    /// The whole log, which reaches `last_seq`, which a subscription fills from [`first_live`] on.
    pub fn new(last_seq: Seq) -> Self {
        let focus = seq_index(last_seq).saturating_sub(1);
        Self { last: last_seq, received: first_live(last_seq) - 1, focus, ..Default::default() }
    }

    /// The events from one agent or subagent, which reach the place `last`, which a subscription fills from
    /// [`first_live`] on.
    pub fn by_place(last: u64) -> Self {
        let focus = seq_index(last).saturating_sub(1);
        Self { by: By::Place, last, received: first_live(last) - 1, focus, ..Default::default() }
    }

    pub fn by(&self) -> By {
        self.by
    }

    pub fn row_count(&self) -> usize {
        seq_index(self.last)
    }

    /// The position of the newest event.
    pub fn last(&self) -> u64 {
        self.last
    }

    pub fn row(&self, index: usize) -> Row<'_> {
        match self.events.get(&(index as u64 + 1)) {
            Some(event) => Row::Event(event),
            None => Row::Unloaded,
        }
    }

    /// The row of the event at `seq`, if this log has it there: in the whole log, its row by `seq`, and otherwise
    /// its row if it's loaded.
    pub fn row_of(&self, seq: Seq) -> Option<usize> {
        let position = match self.by {
            By::Seq => (seq >= 1 && seq <= self.last).then_some(seq)?,
            By::Place => *self.positions.get(&seq)?,
        };
        Some(seq_index(position) - 1)
    }

    /// Where `event` goes in this log: its `seq`, or its place.
    pub fn position(&self, event: &SessionEvent) -> u64 {
        match self.by {
            By::Seq => event.seq,
            By::Place => event.place.get(),
        }
    }

    /// Holds `event`. A position only ever has the one event, so one put in again replaces itself.
    fn insert(&mut self, event: SessionEvent) {
        let position = self.position(&event);
        if self.by == By::Place {
            self.positions.insert(event.seq, position);
        }
        self.events.insert(position, event);
    }

    pub fn tail(&self) -> &Tail {
        &self.tail
    }

    /// The prompts sent and not yet recorded, in the order they were sent, which the tail draws after
    /// [`SessionLog::waiting_prompts`].
    pub fn pending(&self) -> &[QueuedPrompt] {
        &self.pending
    }

    /// The newest event the subscription has delivered, so a new one carries on after it.
    pub(crate) fn received(&self) -> u64 {
        self.received
    }

    /// Takes an event from a live subscription.
    pub fn apply_event(&mut self, event: SessionEvent) {
        if let EventBody::UserPrompt { client_msg_id: Some(id), .. } = &event.body {
            self.pending.retain(|prompt| prompt.client_msg_id != *id);
        }
        let waited = self.at_the_end();
        self.tail.on_event(event.seq, event.run, &event.body);
        self.outline.saw(&event);
        let position = self.position(&event);
        self.received = self.received.max(position);
        self.last = self.last.max(position);
        if matches!(event.body, EventBody::TurnEnded { .. }) {
            self.changes.turns_ended += 1;
        }
        // Its own row, and everything after it, up to the tail.
        self.touched(seq_index(position) - 1..self.row_count() + 1);
        if let Some((prompt, at)) = SettledAt::of(&event) {
            self.settle(prompt, at);
        }
        self.moved_from_the_end(&waited);
        self.keep_live(position, event);
    }

    pub(crate) fn apply_tail(&mut self, tail: Tail) {
        self.tail = tail;
        self.touched(self.row_count()..self.row_count() + 1);
    }

    pub(crate) fn apply_delta(&mut self, delta: &StreamDelta) {
        self.tail.apply(delta);
        self.touched(self.row_count()..self.row_count() + 1);
    }

    /// Adds a prompt that's on its way, so it shows straight away, in the tail. Recording it removes this.
    pub(crate) fn add_pending(&mut self, prompt: QueuedPrompt) {
        self.pending.push(prompt);
        self.touched(self.row_count()..self.row_count() + 1);
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
    pub(crate) fn remove_pending(&mut self, client_msg_id: &ClientMessageId) -> Option<QueuedPrompt> {
        let at = self.pending.iter().position(|prompt| prompt.client_msg_id == *client_msg_id)?;
        let removed = self.pending.remove(at);
        self.touched(self.row_count()..self.row_count() + 1);
        Some(removed)
    }

    /// The rows that are loaded, as runs in order: each event held. Every other row is [`Row::Unloaded`].
    pub fn loaded_rows(&self) -> Vec<Range<usize>> {
        let mut runs: Vec<Range<usize>> = Vec::new();
        let recorded = seq_index(self.last);
        let events = self.events.keys().map(|position| seq_index(*position) - 1).filter(|row| *row < recorded);
        for row in events {
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

    /// The events that are loaded, oldest first.
    pub fn loaded(&self) -> impl Iterator<Item = &SessionEvent> {
        self.events.values()
    }

    /// Whether every event it follows is loaded, so looking through them looks through it all. One that doesn't know
    /// yet how far its events go, as one by place doesn't until the hub says, holds none.
    pub fn holds_every_event(&self) -> bool {
        self.last > 0 && self.events.len() as u64 == self.last
    }

    /// Whether every event at `positions` that the log goes to is loaded.
    pub fn holds(&self, positions: Range<u64>) -> bool {
        let positions = positions.start..positions.end.min(self.last + 1);
        self.events.range(positions.clone()).count() as u64 == positions.end.saturating_sub(positions.start)
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
