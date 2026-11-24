use super::{approvals, outline, pages, position, prompts};
use pages::before_live;
use position::Position;
use prompts::Settled;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Place, QueuedPrompt, Seq, SettledAt};
use protocol::tail::{StreamDelta, Tail};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::sync::Arc;

/// Target for cached events. The viewport and live tail stay loaded even when they need more: a window can span
/// arbitrarily many log positions, since many events have no height.
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

/// A copy shares what's large with the log it was copied from, until one of them changes it: each event, which never
/// changes, the outline's lines, where each prompt was settled and the tail. So a copy costs little however long the
/// chat is, and [`crate::OpenSession::version`] makes one whenever a version is read and the log then changes.
#[derive(Clone, Debug, Default)]
pub struct SessionLog {
    /// The position of the newest event, or 0 when it has none.
    pub(super) last: Position,
    /// The newest event the subscription has delivered, or 0 for none. It delivers every page after this, so those
    /// aren't fetched.
    pub(super) received: Position,
    /// What's loaded, by position.
    pub(super) events: BTreeMap<Position, Arc<SessionEvent>>,
    /// The position of each event loaded, by its `seq`.
    pub(super) positions: HashMap<Seq, Position>,
    /// The pages loaded, as `pages::page_of` numbers them.
    pub(super) pages: BTreeSet<usize>,
    /// The pages loaded that are to be fetched again, as [`SessionLog::read_again`] says, and stay drawn until then.
    pub(super) stale: BTreeSet<usize>,
    pub(super) in_flight: BTreeSet<usize>,
    /// Pages whose fetch failed, which aren't asked for again until [`SessionLog::retry_failed`].
    pub(super) failed: BTreeSet<usize>,
    /// The whole range the viewport needs, including its buffer. It starts at the last row, which comes live.
    pub(super) focus: Range<usize>,
    /// The lowest row this frame couldn't draw. Retaining loaded rows does not request more history.
    pub(super) unfetched: Option<usize>,
    /// Sent, and not yet recorded. Each is held as the hub holds a queued prompt, under the ID it'll be recorded under.
    pub(super) pending: Vec<QueuedPrompt>,
    pub(super) tail: Arc<Tail>,
    pub(super) changes: Changes,
    /// How many times events have been put in, live or by the page.
    pub(super) events_added: u64,
    pub(super) outline: outline::Outline,
    pub(super) settled: Settled,
    /// The requests still waiting on an answer that no page loaded here holds.
    pub(super) apart: approvals::Apart,
}

impl SessionLog {
    /// The events from one agent or subagent, which reach the place `last`, which a subscription fills from after
    /// [`before_live`]. It looks at its last row, or with none, at where its first will be.
    pub fn new(last: Place) -> Self {
        let last = Position::of_place(last);
        let focus = last.row().unwrap_or(0);
        Self { last, received: before_live(last), focus: focus..focus + 1, ..Default::default() }
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
            Some(event) => Row::Event(event.as_ref()),
            None => Row::Unloaded,
        }
    }

    /// The row of the event at `seq`, if it's loaded.
    pub fn row_of(&self, seq: Seq) -> Option<usize> {
        self.position_of(seq)?.row()
    }

    /// Where the event at `seq` is, if this log has it there, as [`SessionLog::row_of`] says.
    pub(super) fn position_of(&self, seq: Seq) -> Option<Position> {
        self.positions.get(&seq).copied()
    }

    /// Holds `event`. A position only ever has the one event, so one put in again replaces itself.
    pub(super) fn insert(&mut self, event: SessionEvent) {
        let position = Position::of(&event);
        self.positions.insert(event.seq, position);
        self.events.insert(position, Arc::new(event));
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
        let position = Position::of(&event);
        // One at 0 has no row. A hub places every event from 1, and an open session refuses one it sends at 0.
        let Some(row) = position.row() else { return };
        if let EventBody::UserPrompt { client_msg_id: Some(id), .. } = &event.body {
            self.pending.retain(|prompt| prompt.client_msg_id != *id);
        }
        let waited = self.at_the_end();
        Arc::make_mut(&mut self.tail).on_event(event.seq, event.run, &event.body);
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
        self.tail = Arc::new(tail);
        self.touched(self.row_count()..self.row_count() + 1);
    }

    pub(crate) fn apply_delta(&mut self, delta: &StreamDelta) {
        Arc::make_mut(&mut self.tail).apply(delta);
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
        self.events.values().map(Arc::as_ref)
    }

    /// Whether every event it follows is loaded, so looking through them looks through it all. One that doesn't know
    /// yet how far its events go, as it doesn't until the hub says, holds none.
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
