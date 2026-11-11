//! What a client holds of one session: a window of its event log, or of the events from one agent or subagent in
//! it, never all of it.
//!
//! No IO and no UI. A view asks for rows by index, this says what to fetch, and whoever owns the
//! connection does the fetching. Rows are indexed by each event's position less one, followed by prompts still
//! waiting to be recorded, so an index means the same thing however much is loaded. An event's position is its
//! `seq` in the whole log, and its place in the events from one agent or subagent.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use protocol::attachment::Attachment;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{ClientMessageId, Seq, seq_index};
use protocol::tail::{StreamDelta, Tail};

mod approvals;
mod outline;
mod prompts;
mod restart;

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
    pending: Vec<PendingPrompt>,
    tail: Tail,
    changes: Changes,
    /// How many times events have been put in, live or by the page.
    events_added: u64,
    outline: outline::Outline,
}

fn page_of(position: u64) -> u64 {
    (position.max(1) - 1) / PAGE_EVENTS
}

/// Where a subscription starts for a log that reaches `last`: the start of the page before the newest,
/// so every page it delivers arrives whole, and an open approval, which is always near the end, comes too.
pub(crate) fn first_live(last: u64) -> u64 {
    page_range(page_of(last).saturating_sub(1)).start
}

fn page_range(page: u64) -> Range<u64> {
    page * PAGE_EVENTS + 1..(page + 1) * PAGE_EVENTS + 1
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
        seq_index(self.last) + self.pending.len()
    }

    /// The position of the newest event.
    pub fn last(&self) -> u64 {
        self.last
    }

    pub fn row(&self, index: usize) -> Row<'_> {
        let position = index as u64 + 1;
        if position <= self.last {
            return match self.events.get(&position) {
                Some(event) => Row::Event(event),
                None => Row::Unloaded,
            };
        }
        match self.pending.get(index - seq_index(self.last)) {
            Some(prompt) => Row::Pending(prompt),
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

    /// The newest event the subscription has delivered, so a new one carries on after it.
    pub(crate) fn received(&self) -> u64 {
        self.received
    }

    /// Notes the row a view wants, which decides what to fetch and what stays loaded.
    pub(crate) fn focus_on(&mut self, row: usize) {
        self.focus = row;
    }

    /// Pages to fetch for the row a view wants, nearest first, marking them as in flight. A page the
    /// subscription has yet to deliver isn't one of them.
    pub(crate) fn pages_to_fetch(&mut self) -> Vec<Range<u64>> {
        if self.last == 0 {
            return Vec::new();
        }
        let focus = self.focus.min(seq_index(self.last) - 1) as u64 + 1;
        let center = page_of(focus);
        let last = page_of(self.last);
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
    pub(crate) fn install_page(&mut self, start: u64, events: Vec<SessionEvent>) {
        let page = page_of(start);
        self.in_flight.remove(&page);
        self.pages.insert(page);
        let range = page_range(page);
        for event in events {
            self.last = self.last.max(self.position(&event));
            self.outline.saw(&event);
            self.insert(event);
        }
        self.events_added += 1;
        self.evict();
        self.touched(seq_index(range.start - 1)..seq_index((range.end - 1).min(self.last)));
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
        self.tail.on_event(event.seq, event.run, &event.body);
        self.outline.saw(&event);
        let position = self.position(&event);
        self.received = self.received.max(position);
        self.last = self.last.max(position);
        if matches!(event.body, EventBody::TurnEnded { .. }) {
            self.changes.turns_ended += 1;
        }
        // Its own row, and everything after it, which a prompt it recorded may have moved, up to the tail.
        self.touched(seq_index(position) - 1..self.row_count() + 1);
        // A prompt the agent dropped says so under it.
        if let EventBody::PromptDropped { prompt } = &event.body
            && let Some(row) = self.row_of(*prompt)
        {
            self.touched(row..row + 1);
        }
        // Only the tail stays loaded from live events. Anything older is fetched when scrolled to.
        if page_of(position) + 1 >= page_of(self.last) {
            self.pages.insert(page_of(position));
            self.insert(event);
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
        self.touched(seq_index(self.last)..self.row_count() + 1);
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
        self.touched(seq_index(self.last)..self.row_count() + 1);
        Some(removed)
    }

    /// Drops whole pages furthest from where a view is looking, keeping the tail for live updates.
    fn evict(&mut self) {
        let center = page_of(self.focus as u64 + 1);
        let tail = page_of(self.last);
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
            let doomed: Vec<u64> = self.events.range(range).map(|(position, _)| *position).collect();
            for position in doomed {
                if let Some(event) = self.events.remove(&position) {
                    self.positions.remove(&event.seq);
                }
            }
        }
    }

    /// The rows that are loaded, as runs in order: each event held, and every prompt sent and not yet recorded.
    /// Every other row is [`Row::Unloaded`].
    pub fn loaded_rows(&self) -> Vec<Range<usize>> {
        let mut runs: Vec<Range<usize>> = Vec::new();
        let recorded = seq_index(self.last);
        let events = self.events.keys().map(|position| seq_index(*position) - 1).filter(|row| *row < recorded);
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
