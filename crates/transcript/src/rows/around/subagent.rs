//! What a subagent has done, gathered from the log for the call that started it. Its events arrive in rows of their
//! own, after the call and among the agent's other work and any other subagent's, so the call gathers them and their
//! own rows are empty. One too far after its call to be gathered is drawn where it is instead, so nothing the
//! subagent did goes missing.

use std::collections::{HashMap, HashSet};

use client_core::session_log::{MAX_LOADED_EVENTS, SessionLog};
use protocol::event::{ContentBlock, EventBody, SessionEvent, SubagentOutcome};
use protocol::model::{Seq, ToolUseId, seq_index};

use super::{Around, blocks_of, nearby};
use crate::calls::agent;
use crate::transcript::Model;

/// How far after its call a subagent's events are looked for: as many as the log holds at once. A subagent can work
/// for hundreds of events, and the agent goes on with its own work alongside it.
const REACH: usize = MAX_LOADED_EVENTS;

/// What a subagent has done so far.
pub(in crate::rows) struct Subagent {
    /// The call that started it.
    pub(in crate::rows) call: String,
    /// How many calls it's made.
    pub(in crate::rows) calls: usize,
    /// Its events, with what each needs to be drawn. Only gathered while its call is open.
    pub(in crate::rows) events: Option<Vec<(SessionEvent, Around)>>,
    /// When the agent said it started.
    pub(in crate::rows) started_at_ms: Option<i64>,
    /// How it ended, once the agent said.
    pub(in crate::rows) ended: Option<Ended>,
}

/// How a subagent ended.
pub(in crate::rows) struct Ended {
    pub(in crate::rows) outcome: SubagentOutcome,
    pub(in crate::rows) report: Option<String>,
    pub(in crate::rows) at_ms: i64,
}

impl Subagent {
    /// What the subagent started by the call `id`, in the event at `seq`, has done.
    pub(in crate::rows) fn read(log: &SessionLog, m: &Model, seq: Seq, id: &str, open: bool) -> Self {
        let mut subagent =
            Self { call: id.to_string(), calls: 0, events: open.then(Vec::new), started_at_ms: None, ended: None };
        for event in nearby::events_after(log, seq, REACH) {
            if let EventBody::SubagentStarted { call, .. } = &event.body
                && call.as_str() == id
            {
                subagent.started_at_ms = Some(event.ts_ms);
            }
            if let EventBody::SubagentEnded { call, outcome, report } = &event.body
                && call.as_str() == id
            {
                subagent.ended = Some(Ended { outcome: *outcome, report: report.clone(), at_ms: event.ts_ms });
            }
            if !parent_of(&event.body).is_some_and(|parent| parent.as_str() == id) {
                continue;
            }
            let blocks = blocks_of(&event.body);
            subagent.calls += blocks.iter().filter(|block| matches!(block, ContentBlock::ToolUse { .. })).count();
            if let Some(events) = &mut subagent.events {
                events.push((event.clone(), Around::read(log, m, event.seq, &event.body)));
            }
        }
        subagent
    }
}

/// Whether `event` is a subagent's, and drawn in the call that started it, so its own row is empty. It is when the
/// call is close enough before it to have gathered it. Otherwise, such as when the call is further back than the log
/// holds, it's drawn where it is, so it shows somewhere.
pub(in crate::rows) fn drawn_in_its_call(log: &SessionLog, m: &Model, event: &SessionEvent) -> bool {
    let Some(parent) = parent_of(&event.body) else { return false };
    let Some(call) = m.subagent_calls.borrow_mut().find(log, event.seq, parent) else { return false };
    // The same reach as `Subagent::read`, which gathers the events after its call's row.
    call < event.seq && seq_index(event.seq - call) <= REACH
}

/// Where the calls that started subagents are, as rows have looked for them. A subagent's events, which can fill a
/// screen, each look for theirs on every frame.
#[derive(Default)]
pub(crate) struct SubagentCalls {
    /// Each call found, by its ID. A call never moves, but the log can let go of its page, and then it's forgotten.
    found: HashMap<ToolUseId, Seq>,
    /// The events whose call wasn't within reach before them, by seq. The log may have the call once it has more
    /// events, so these are looked at again then.
    missed: HashSet<Seq>,
    /// How many times the log had had events added when `missed` was last emptied.
    added: u64,
}

impl SubagentCalls {
    /// Where the call `id` is, among the events within reach before the one at `seq`.
    fn find(&mut self, log: &SessionLog, seq: Seq, id: &ToolUseId) -> Option<Seq> {
        match self.found.get(id) {
            Some(&call) if nearby::is_loaded(log, call) => return Some(call),
            // Its page was let go, so it isn't drawn, and its subagent's events have to be drawn where they are.
            Some(_) => {
                self.found.remove(id);
            }
            None => {}
        }
        if self.added != log.events_added() {
            self.missed.clear();
            self.added = log.events_added();
        }
        if self.missed.contains(&seq) {
            return None;
        }
        let found = nearby::call_before(log, seq, id.as_str(), REACH);
        let found = found.filter(|(_, name)| agent::hands_off(name)).map(|(call, _)| call);
        match found {
            Some(call) => {
                self.found.insert(id.clone(), call);
            }
            None => {
                self.missed.insert(seq);
            }
        }
        found
    }
}

/// The call that started the subagent an event is from, if it's from one.
fn parent_of(body: &EventBody) -> Option<&ToolUseId> {
    let (EventBody::Assistant { parent_tool_use_id, .. } | EventBody::AgentUser { parent_tool_use_id, .. }) = body
    else {
        return None;
    };
    parent_tool_use_id.as_ref()
}

#[cfg(test)]
mod tests {
    use protocol::model::Seq;

    use super::nearby::is_loaded;
    use super::{Subagent, drawn_in_its_call};
    use crate::fixtures::{calls, calls_to, event, log, result, within};
    use crate::transcript::Model;

    #[test]
    fn a_subagent_s_event_out_of_its_call_s_reach_is_drawn_where_it_is() {
        let report = |seq| event(seq, within("agent", calls(&["report"])));
        let log = log([event(1, calls_to("Agent", &["agent"])), report(2), report(2001), report(2002)]);
        let m = Model::default();

        assert!(drawn_in_its_call(&log, &m, &report(2)));
        assert!(drawn_in_its_call(&log, &m, &report(2001)), "the last event the call gathers");
        assert!(!drawn_in_its_call(&log, &m, &report(2002)), "too far for the call to gather");
        let orphan = event(3, within("a call that isn't here", calls(&["x"])));
        assert!(!drawn_in_its_call(&log, &m, &orphan), "with no call to draw it");
        let gathered = Subagent::read(&log, &m, 1, "agent", true).events.unwrap();
        let seqs: Vec<Seq> = gathered.iter().map(|(event, _)| event.seq).collect();
        assert_eq!(seqs, [2, 2001], "the call gathers exactly what's drawn in it");
    }

    #[test]
    fn a_call_not_found_is_looked_for_again_once_the_log_has_more() {
        let work = event(5, within("agent", calls(&["a"])));
        let mut log = log([work.clone()]);
        let m = Model::default();
        assert!(!drawn_in_its_call(&log, &m, &work), "its call isn't loaded");

        log.apply_event(event(1, calls_to("Agent", &["agent"])));

        assert!(drawn_in_its_call(&log, &m, &work));
    }

    #[test]
    fn a_call_the_log_lets_go_of_no_longer_draws_its_subagent_s_work() {
        let work = |seq| event(seq, within("agent", calls(&["work"])));
        let mut log = log((1..=1500).map(|seq| event(seq, result("x"))));
        log.apply_event(event(1501, calls_to("Agent", &["agent"])));
        log.apply_event(work(1502));
        let m = Model::default();
        assert!(drawn_in_its_call(&log, &m, &work(1502)), "found, and remembered");

        for seq in 1503..=2500 {
            log.apply_event(event(seq, result("x")));
        }
        log.apply_event(work(2501));

        assert!(!is_loaded(&log, 1501), "the log has let go of the call's page");
        assert!(!drawn_in_its_call(&log, &m, &work(2501)), "drawn where it is, since its call isn't");
    }

    #[test]
    fn only_a_call_that_hands_off_draws_a_subagent_s_work() {
        let work = event(2, within("not-an-agent", calls(&["a"])));
        let log = log([event(1, calls(&["not-an-agent"])), work.clone()]);

        assert!(!drawn_in_its_call(&log, &Model::default(), &work));
    }

    #[test]
    fn a_subagent_s_work_is_gathered_from_among_everything_else() {
        let events = [
            calls_to("Agent", &["agent"]),
            within("agent", calls(&["a"])),
            calls(&["the agent's own"]),
            within("another", calls(&["b"])),
            within("agent", calls(&["c"])),
        ];
        let log = log((1..).zip(events).map(|(seq, body)| event(seq, body)));
        let m = Model::default();

        let open = Subagent::read(&log, &m, 1, "agent", true);
        let closed = Subagent::read(&log, &m, 1, "agent", false);

        assert_eq!(open.calls, 2);
        let seqs: Vec<Seq> = open.events.unwrap().iter().map(|(event, _)| event.seq).collect();
        assert_eq!(seqs, [2, 5], "only its own events, in order");
        assert_eq!(closed.calls, 2);
        assert!(closed.events.is_none(), "a closed call only counts");
    }
}
