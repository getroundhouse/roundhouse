//! Where each of the log's rows is drawn in the transcript's list. A row is drawn where its event arrived, except a
//! subagent's, which is drawn straight after the call that started it, with the rest of its work in the order it
//! arrived. That work comes mixed with the agent's own and with other subagents', and this way it reads as one story
//! under its call. While the call is closed, its work isn't in the list at all.
//!
//! Each event keeps a row of its own because a virtual list draws only the rows on screen, but each of those whole, so
//! work drawn inside its call's row was all drawn on every frame. Work left in the list while its call is closed would
//! all be drawn whenever the call is, since rows with nothing in them take no room. `docs/performance.md` has what each
//! cost. Work too far after its call to be gathered, or whose call isn't loaded, stays where it arrived, so it shows
//! somewhere.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use client_core::session_log::{MAX_LOADED_EVENTS, SessionLog};
use protocol::event::ContentBlock;
use protocol::model::{Seq, ToolUseId, seq_index};

use crate::calls::agent;
use crate::folds::{Fold, Folds};
use crate::item_key::ItemKey;

#[cfg(test)]
mod laws;
#[cfg(test)]
mod tests;

/// How far after its call a subagent's events are gathered: as many as the log holds at once. A subagent can work
/// for hundreds of events, and the agent goes on with its own work alongside it.
const REACH: usize = MAX_LOADED_EVENTS;

/// A row of the session's log: the event at `seq - 1`, or a prompt still being sent, after the events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct LogRow(pub(crate) usize);

/// Where something is drawn in the transcript's list: the log's rows, in the placement's order, then the streaming
/// tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Position(pub(crate) usize);

#[derive(Default)]
pub(crate) struct Placement {
    /// The log's rows the list draws, in order.
    rows: Vec<LogRow>,
    /// Where each of the log's rows is drawn, by row. `None` is work under a closed call.
    positions: Vec<Option<Position>>,
    /// For each row of a subagent's work: the call that started the subagent.
    within: HashMap<LogRow, ToolUseId>,
    /// Each call that started a subagent with work to draw after it.
    calls: HashMap<ToolUseId, Call>,
    /// The calls in each row that have work to draw after them, in the order they're in the row.
    starts: HashMap<LogRow, Vec<ToolUseId>>,
    /// What the log held, and how many blocks had been opened or closed, when this was worked out. Nothing else
    /// changes where rows go.
    seen: Seen,
}

/// A call that started a subagent, and what the subagent has done as far as the log holds.
#[derive(Clone, Debug)]
pub(crate) struct Call {
    /// The row of the event the call is in.
    pub(crate) row: LogRow,
    /// That event.
    pub(crate) seq: Seq,
    /// The call's block, which opens and closes, and whose text keys the subagent's report.
    pub(crate) key: ItemKey,
    /// Where the call is among the event's blocks.
    position: usize,
    /// Whether it's open, so its work is drawn.
    pub(crate) open: bool,
    pub(crate) work: Work,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Work {
    /// How many calls the subagent has made.
    pub(crate) calls: usize,
    /// Whether it wrote anything. Its last words are its report.
    pub(crate) said: bool,
    /// The row drawn last under the call, where its rule ends: the subagent's last event, or the last of the work of an
    /// open subagent it started. `None` while none of its work is drawn, because it or a call it's under is closed.
    pub(crate) last: Option<LogRow>,
}

/// How the list has to change to match where the log's rows go now.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Update {
    /// To make in order.
    pub(crate) splices: Vec<Splice>,
    /// Once they're made, the rows that stay but are drawn differently because of others, to measure again: each call
    /// whose subagent's work started, grew, opened or closed, and the rows that end its rule, before and after.
    pub(crate) restyled: Vec<Range<Position>>,
}

/// At `at`, `removed` rows of the list go, and `added` new ones take their place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Splice {
    pub(crate) at: Position,
    pub(crate) removed: usize,
    pub(crate) added: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Seen {
    rows: usize,
    /// The log lets go of events only as it adds others, so this changes whenever what it holds does.
    added: u64,
    chosen: u64,
}

impl Seen {
    fn of(log: &SessionLog, folds: &Folds) -> Self {
        Self { rows: log.row_count(), added: log.events_added(), chosen: folds.chosen_count() }
    }
}

impl Placement {
    /// Where the log's rows go, as it holds them now, with the calls open that `folds` says are.
    pub(crate) fn of(log: &SessionLog, folds: &Folds) -> Self {
        let seen = Seen::of(log, folds);
        let (within, mut calls) = gather(log, folds);
        let mut starts: HashMap<LogRow, Vec<ToolUseId>> = HashMap::new();
        for (id, call) in &calls {
            starts.entry(call.row).or_default().push(id.clone());
        }
        for ids in starts.values_mut() {
            ids.sort_by_key(|id| calls[id].position);
        }
        let rows = order(seen.rows, &within, &calls, &starts);
        let mut positions = vec![None; seen.rows];
        for (position, row) in rows.iter().enumerate() {
            positions[row.0] = Some(Position(position));
        }
        // Rows are in order, so the last a call's work reaches is the last one that names it, or names a call in it.
        for &row in &rows {
            let mut call = within.get(&row);
            while let Some(id) = call {
                let Some(started) = calls.get_mut(id) else { break };
                started.work.last = Some(row);
                call = within.get(&started.row);
            }
        }
        Self { rows, positions, within, calls, starts, seen }
    }

    /// Moves to where the log's rows go now, and says how the list has to change to match. Only rows that are new,
    /// gone or drawn somewhere else are spliced. Every other row keeps its place among the others, and isn't touched,
    /// so the list keeps what it measured and where it's scrolled to.
    pub(crate) fn follow(&mut self, log: &SessionLog, folds: &Folds) -> Update {
        if Seen::of(log, folds) == self.seen {
            return Update::default();
        }
        let next = Self::of(log, folds);
        let update = Update { splices: self.splices_to(&next), restyled: self.restyled_in(&next) };
        *self = next;
        update
    }

    /// How many rows the list draws for the log.
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    /// Where the streaming tail is drawn: after the log's rows.
    pub(crate) fn tail(&self) -> Position {
        Position(self.rows.len())
    }

    /// The log's row drawn at `position`, or `None` for the streaming tail after them.
    pub(crate) fn row_at(&self, position: Position) -> Option<LogRow> {
        self.rows.get(position.0).copied()
    }

    /// Where `row` is drawn, if it is.
    pub(crate) fn position_of(&self, row: LogRow) -> Option<Position> {
        self.positions.get(row.0).copied().flatten()
    }

    /// Where these rows are drawn, as runs of positions, leaving out any that aren't.
    pub(crate) fn positions_of(&self, rows: impl IntoIterator<Item = LogRow>) -> Vec<Range<Position>> {
        let mut positions: Vec<Position> = rows.into_iter().filter_map(|row| self.position_of(row)).collect();
        positions.sort_unstable();
        positions.dedup();
        runs(&positions)
    }

    /// The call that started the subagent `row` is the work of, when it's drawn after that call.
    pub(crate) fn within(&self, row: LogRow) -> Option<&ToolUseId> {
        self.within.get(&row)
    }

    /// The call `id`, if it started a subagent with work to draw after it.
    pub(crate) fn call(&self, id: &ToolUseId) -> Option<&Call> {
        self.calls.get(id)
    }

    /// Where `row` is drawn, and the work drawn after it of each subagent its open calls started, all of which changes
    /// height when one of those calls opens or closes. `None` if `row` isn't drawn.
    pub(crate) fn drawn_with(&self, row: LogRow) -> Option<Range<Position>> {
        let start = self.position_of(row)?;
        let calls = self.starts.get(&row).into_iter().flatten().map(|id| &self.calls[id]);
        let last = calls.filter_map(|call| call.work.last.and_then(|last| self.position_of(last))).max();
        Some(start..Position(last.unwrap_or(start).0 + 1))
    }

    fn splices_to(&self, next: &Placement) -> Vec<Splice> {
        let (mut removed, mut added) = (Vec::new(), Vec::new());
        let mut memo = HashMap::new();
        for row in self.rows_that_can_move(next) {
            match (self.position_of(row), next.position_of(row)) {
                (Some(was), Some(is)) if self.moves(next, row, &mut memo) => {
                    removed.push(was);
                    added.push(is);
                }
                (Some(_), Some(_)) | (None, None) => {}
                (Some(was), None) => removed.push(was),
                (None, Some(is)) => added.push(is),
            }
        }
        removed.sort_unstable();
        added.sort_unstable();
        // From the bottom, so the rows above are where they were for each.
        let removals =
            runs(&removed).into_iter().rev().map(|run| Splice { at: run.start, removed: len(&run), added: 0 });
        // From the top, so each lands among the rows that come before it.
        let additions = runs(&added).into_iter().map(|run| Splice { at: run.start, removed: 0, added: len(&run) });
        removals.chain(additions).collect()
    }

    /// The only rows that can be drawn somewhere else in `next`: a subagent's work in either, and rows only one has.
    fn rows_that_can_move(&self, next: &Placement) -> Vec<LogRow> {
        let (had, has) = (self.positions.len(), next.positions.len());
        let mut rows: Vec<LogRow> = self.within.keys().chain(next.within.keys()).copied().collect();
        rows.extend((had.min(has)..had.max(has)).map(LogRow));
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    /// Whether `row`, drawn in both, is under different calls in `next`, or under a call that moved. Any other row keeps
    /// its place among the rest.
    fn moves(&self, next: &Placement, row: LogRow, memo: &mut HashMap<LogRow, bool>) -> bool {
        if let Some(&known) = memo.get(&row) {
            return known;
        }
        let moves = match (self.within.get(&row), next.within.get(&row)) {
            (None, None) => false,
            // Under the same call, it moves when the call does.
            (Some(was), Some(is)) if was == is => self.moves(next, self.calls[was].row, memo),
            (Some(_) | None, Some(_) | None) => true,
        };
        memo.insert(row, moves);
        moves
    }

    /// The rows in `next` whose look depends on a call's work that changed: the call's row, which ends its rule only
    /// while no work follows it, and the rows that end the rule before and after. That includes a call whose work the
    /// log no longer holds, or holds out of its reach.
    fn restyled_in(&self, next: &Placement) -> Vec<Range<Position>> {
        let ids: HashSet<&ToolUseId> = self.calls.keys().chain(next.calls.keys()).collect();
        let mut rows = Vec::new();
        for id in ids {
            let (was, is) = (self.calls.get(id), next.calls.get(id));
            if let (Some(was), Some(is)) = (was, is)
                && (was.open, &was.work) == (is.open, &is.work)
            {
                continue;
            }
            rows.extend([was, is].into_iter().flatten().flat_map(|call| [Some(call.row), call.work.last]).flatten());
        }
        next.positions_of(rows)
    }
}

/// Which events are a subagent's work, drawn after its call: each whose call is loaded, before it, and near enough.
fn gather(log: &SessionLog, folds: &Folds) -> (HashMap<LogRow, ToolUseId>, HashMap<ToolUseId, Call>) {
    let (mut within, mut calls) = (HashMap::new(), HashMap::<ToolUseId, Call>::new());
    // Every call that starts a subagent, with work or not yet.
    let mut started: HashMap<ToolUseId, Call> = HashMap::new();
    for event in log.loaded() {
        let row = LogRow(seq_index(event.seq) - 1);
        let blocks = event.body.blocks();
        if let Some(parent) = event.body.parent_tool_use_id()
            && let Some(call) = started.get(parent)
            && row.0 - call.row.0 <= REACH
        {
            within.insert(row, parent.clone());
            let call = calls.entry(parent.clone()).or_insert_with(|| Call { work: Work::default(), ..call.clone() });
            call.work.calls += blocks.iter().filter(|block| matches!(block, ContentBlock::ToolUse { .. })).count();
            call.work.said |= blocks.iter().any(|block| matches!(block, ContentBlock::Text { .. }));
        }
        for (position, block) in blocks.iter().enumerate() {
            if let ContentBlock::ToolUse { id, name, .. } = block
                && agent::hands_off(name)
            {
                let key = ItemKey::of_block(event.seq, &event.body, position);
                let open = folds.is_open(&key, Fold::of(block));
                let call = Call { row, seq: event.seq, key, position, open, work: Work::default() };
                started.insert(ToolUseId::from(id.clone()), call);
            }
        }
    }
    (within, calls)
}

/// The rows the list draws, in order: each where it arrived, except a subagent's work, which follows its call while the
/// call is open, and isn't drawn while it's closed. A row with several such calls has each one's work after it in
/// turn, and work that started a subagent of its own is followed by that one's.
fn order(
    rows: usize,
    within: &HashMap<LogRow, ToolUseId>,
    calls: &HashMap<ToolUseId, Call>,
    starts: &HashMap<LogRow, Vec<ToolUseId>>,
) -> Vec<LogRow> {
    if within.is_empty() {
        return (0..rows).map(LogRow).collect();
    }
    let mut work: HashMap<&ToolUseId, Vec<LogRow>> = HashMap::new();
    let mut gathered: Vec<LogRow> = within.keys().copied().collect();
    gathered.sort_unstable();
    for row in &gathered {
        work.entry(&within[row]).or_default().push(*row);
    }
    // Only work, and rows with work after them, need more than to go where they arrived. The rest go in as runs.
    let tops = starts.keys().copied().filter(|row| !within.contains_key(row));
    let mut special: Vec<LogRow> = gathered.into_iter().chain(tops).collect();
    special.sort_unstable();
    let (mut order, mut next, mut stack) = (Vec::with_capacity(rows), 0, Vec::new());
    for row in special {
        order.extend((next..row.0).map(LogRow));
        next = row.0 + 1;
        // Work is drawn after its call, if at all.
        if within.contains_key(&row) {
            continue;
        }
        stack.push(row);
        while let Some(row) = stack.pop() {
            order.push(row);
            for id in starts.get(&row).into_iter().flatten().rev().filter(|id| calls[*id].open) {
                stack.extend(work[id].iter().rev());
            }
        }
    }
    order.extend((next..rows).map(LogRow));
    order
}

/// Runs of consecutive positions, from sorted ones.
fn runs(positions: &[Position]) -> Vec<Range<Position>> {
    let mut runs: Vec<Range<Position>> = Vec::new();
    for &position in positions {
        match runs.last_mut() {
            Some(run) if run.end == position => run.end = Position(position.0 + 1),
            _ => runs.push(position..Position(position.0 + 1)),
        }
    }
    runs
}

fn len(run: &Range<Position>) -> usize {
    run.end.0 - run.start.0
}
