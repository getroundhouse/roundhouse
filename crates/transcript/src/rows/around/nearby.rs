//! Finding events near another in the log, for what drawing it needs from other rows, such as a call's result. An
//! event's row is the log's to say, since a log of the agent's own events or one subagent's holds only some `seq`s.

use client_core::session_log::{Row, SessionLog};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::Seq;

/// The events after the one at `seq`, in order, as far as `reach` rows on. A row that isn't loaded is skipped.
pub(super) fn events_after(log: &SessionLog, seq: Seq, reach: usize) -> impl Iterator<Item = &SessionEvent> {
    let after = log.row_of(seq).map_or(log.row_count(), |row| row + 1);
    (after..after.saturating_add(reach).min(log.row_count())).filter_map(|ix| event_in(log, ix))
}

/// The events before the one at `seq`, nearest first, as far as `reach` rows back.
pub(super) fn events_before(log: &SessionLog, seq: Seq, reach: usize) -> impl Iterator<Item = &SessionEvent> {
    let row = log.row_of(seq).unwrap_or(0);
    (row.saturating_sub(reach)..row).rev().filter_map(|ix| event_in(log, ix))
}

/// The call `id`, among the events as far as `reach` rows before the one at `seq`: the seq of the event it's in,
/// and its tool's name.
pub(super) fn call_before<'a>(log: &'a SessionLog, seq: Seq, id: &str, reach: usize) -> Option<(Seq, &'a str)> {
    events_before(log, seq, reach).find_map(|event| {
        let EventBody::Assistant { blocks, .. } = &event.body else { return None };
        blocks.iter().find_map(|block| {
            let ContentBlock::ToolUse { id: call, name, .. } = block else { return None };
            (call == id).then_some((event.seq, name.as_str()))
        })
    })
}

/// Whether the event at `seq` is loaded, so its row is drawn.
pub(super) fn is_loaded(log: &SessionLog, seq: Seq) -> bool {
    log.row_of(seq).is_some_and(|row| event_in(log, row).is_some())
}

fn event_in(log: &SessionLog, ix: usize) -> Option<&SessionEvent> {
    match log.row(ix) {
        Row::Event(event) => Some(event),
        Row::Pending(_) | Row::Unloaded => None,
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::Seq;

    use super::{call_before, events_after, events_before};
    use crate::fixtures::{calls, event, log, result};

    #[test]
    fn nearby_events_go_as_far_as_their_reach() {
        let log = log((1..=5).map(|seq| event(seq, result("x"))));
        let seqs = |events: Vec<&protocol::event::SessionEvent>| events.iter().map(|e| e.seq).collect::<Vec<Seq>>();

        assert_eq!(seqs(events_after(&log, 2, 2).collect()), [3, 4]);
        assert_eq!(seqs(events_after(&log, 4, 10).collect()), [5], "to the end of the log");
        assert_eq!(seqs(events_before(&log, 4, 2).collect()), [3, 2], "nearest first");
        assert_eq!(seqs(events_before(&log, 2, 10).collect()), [1], "to the start of the log");
    }

    #[test]
    fn a_call_is_found_by_its_id_among_the_events_before() {
        let log = log([event(1, calls(&["a"])), event(2, calls(&["b", "c"])), event(3, result("c"))]);

        assert_eq!(call_before(&log, 3, "c", 5), Some((2, "Bash")));
        assert_eq!(call_before(&log, 3, "a", 1), None, "further back than it looks");
        assert_eq!(call_before(&log, 3, "d", 5), None);
    }
}
