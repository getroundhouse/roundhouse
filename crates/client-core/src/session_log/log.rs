use super::{approvals, outline, pages, position, prompts};
use pages::before_live;
use position::Position;
use prompts::Settled;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Place, QueuedPrompt, Seq, SettledAt};
use protocol::tail::{StreamDelta, Tail};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

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
    pub(super) by: By,
    /// The position of the newest event, or 0 when it has none.
    pub(super) last: Position,
    /// The newest event the subscription has delivered, or 0 for none. It delivers every page after this, so those
    /// aren't fetched.
    pub(super) received: Position,
    /// What's loaded, by position.
    pub(super) events: BTreeMap<Position, SessionEvent>,
    /// The position of each event loaded, by its `seq`, when that isn't its position.
    pub(super) positions: HashMap<Seq, Position>,
    /// The pages loaded, as `pages::page_of` numbers them.
    pub(super) pages: BTreeSet<usize>,
    pub(super) in_flight: BTreeSet<usize>,
    /// Pages whose fetch failed, which aren't asked for again until [`SessionLog::retry_failed`].
    pub(super) failed: BTreeSet<usize>,
    /// The row a view last wanted, which decides what stays loaded. It starts at the last row, where a view opens a
    /// chat, so a log no view has asked of yet fetches nothing, since its end comes live.
    pub(super) focus: usize,
    /// Sent, and not yet recorded. Each is held as the hub holds a queued prompt, under the ID it'll be recorded under.
    pub(super) pending: Vec<QueuedPrompt>,
    pub(super) tail: Tail,
    pub(super) changes: Changes,
    /// How many times events have been put in, live or by the page.
    pub(super) events_added: u64,
    pub(super) outline: outline::Outline,
    pub(super) settled: Settled,
    /// The requests still waiting on an answer that no page loaded here holds.
    pub(super) apart: approvals::Apart,
}

impl SessionLog {
    /// The whole log, which reaches `last_seq`, which a subscription fills from after [`before_live`].
    pub fn new(last_seq: Seq) -> Self {
        Self::reaching(By::Seq, Position::of_seq(last_seq))
    }

    /// The events from one agent or subagent, which reach the place `last`, which a subscription fills from after
    /// [`before_live`].
    pub fn by_place(last: Place) -> Self {
        Self::reaching(By::Place, Position::of_place(last))
    }

    /// A log by `by` that reaches `last`, looking at its last row, or with none, at where its first will be.
    pub(super) fn reaching(by: By, last: Position) -> Self {
        let focus = last.row().unwrap_or(0);
        Self { by, last, received: before_live(last), focus, ..Default::default() }
    }

    pub fn by(&self) -> By {
        self.by
    }

    /// A row for each position up to the newest event's.
    pub fn row_count(&self) -> usize {
        self.last.row().map_or(0, |last| last + 1)
    }

    /// The position of the newest event, or 0 when it has none.
    pub fn last(&self) -> Position {
        self.last
    }

    pub fn row(&self, index: usize) -> Row<'_> {
        match self.events.get(&Position::of_row(index)) {
            Some(event) => Row::Event(event),
            None => Row::Unloaded,
        }
    }

    /// The row of the event at `seq`, if this log has it there: in the whole log, its row by `seq`, and otherwise
    /// its row if it's loaded.
    pub fn row_of(&self, seq: Seq) -> Option<usize> {
        self.position_of(seq)?.row()
    }

    /// Where the event at `seq` is, if this log has it there, as [`SessionLog::row_of`] says.
    pub(super) fn position_of(&self, seq: Seq) -> Option<Position> {
        match self.by {
            By::Seq => Some(Position::of_seq(seq)).filter(|position| *position <= self.last),
            By::Place => self.positions.get(&seq).copied(),
        }
    }

    /// Where `event` goes in this log: its `seq`, or its place.
    pub fn position(&self, event: &SessionEvent) -> Position {
        match self.by {
            By::Seq => Position::of_seq(event.seq),
            By::Place => Position::of_place(event.place),
        }
    }

    /// Holds `event`. A position only ever has the one event, so one put in again replaces itself.
    pub(super) fn insert(&mut self, event: SessionEvent) {
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

    /// The newest event the subscription has delivered, or 0 for none, so a new one carries on after it.
    pub(crate) fn received(&self) -> Position {
        self.received
    }

    /// Takes an event from a live subscription.
    pub fn apply_event(&mut self, event: SessionEvent) {
        let position = self.position(&event);
        // One at 0 has no row: a hub numbers events from 1, and one that records no places isn't read by place.
        let Some(row) = position.row() else { return };
        if let EventBody::UserPrompt { client_msg_id: Some(id), .. } = &event.body {
            self.pending.retain(|prompt| prompt.client_msg_id != *id);
        }
        let waited = self.at_the_end();
        self.tail.on_event(event.seq, event.run, &event.body);
        self.outline.saw(&event);
        self.received = self.received.max(position);
        self.last = self.last.max(position);
        if matches!(event.body, EventBody::TurnEnded { .. }) {
            self.changes.turns_ended += 1;
        }
        // Its own row, and everything after it, up to the tail.
        self.touched(row..self.row_count() + 1);
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

    pub(super) fn touched(&mut self, rows: Range<usize>) {
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
        let recorded = self.row_count();
        let events = self.events.keys().filter_map(|position| position.row()).filter(|row| *row < recorded);
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
        self.row_count() > 0 && self.events.len() == self.row_count()
    }

    /// Whether every one of `rows` that the log goes to is loaded.
    pub fn holds(&self, rows: Range<usize>) -> bool {
        let rows = rows.start..rows.end.min(self.row_count());
        rows.is_empty() || self.events.range(Position::of_rows(rows.clone())).count() == rows.len()
    }

    /// How many times events have been added to what's held. It changes whenever looking through the events again
    /// could find something that wasn't there before, so a view can keep what it found until then.
    pub fn events_added(&self) -> u64 {
        self.events_added
    }
}
