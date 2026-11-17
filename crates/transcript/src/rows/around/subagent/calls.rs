use super::super::{Around, BlockAround, Opening, Reply, blocks_of, nearby};
use super::lately;
use crate::transcript::Model;
use client_core::session_log::{By, MAX_LOADED_EVENTS, SessionLog};
use lately::Lately;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Seq, Subagent as Record, SubagentId, SubagentStatus, ToolUseId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// How far after its call a subagent's events are looked for: as many as the log holds at once. A subagent can work
/// for hundreds of events, and the agent goes on with its own work alongside it.
pub(crate) const REACH: usize = MAX_LOADED_EVENTS;

/// What a subagent has done so far.
pub(in crate::rows) struct Subagent {
    /// The call that started it.
    pub(in crate::rows) call: ToolUseId,
    /// How many calls it's made.
    pub(in crate::rows) calls: usize,
    /// Its events, with what each needs to be drawn. Only gathered while its call is open, and never when its work is
    /// kept apart.
    pub(in crate::rows) events: Option<Vec<(SessionEvent, Around)>>,
    /// Its work is kept apart from the chat's, in the subagent tab, so it's drawn from its record.
    pub(in crate::rows) apart: bool,
    /// Its record, once the hub has sent it.
    pub(in crate::rows) record: Option<Arc<Record>>,
    /// From its record, while it works and its call is open: what it's done lately, as blocks, oldest first, each
    /// with what drawing it needs.
    pub(in crate::rows) lately: Vec<(ContentBlock, BlockAround)>,
}

/// Whether subagents' work is kept apart from the chat's, in the subagent tab, so a call about one is drawn from its
/// record: from a hub that sends the chat only the agent's own events, for an agent that records its subagents' work.
/// Antigravity's adapter can't, since its CLI doesn't send it, so its subagents have none kept apart for the tab to
/// show.
pub(super) fn kept_apart(log: &SessionLog, m: &Model) -> bool {
    log.by() == By::Place && m.records_subagents()
}

impl Subagent {
    /// What the subagent started by the call `id`, in the event at `seq`, has done.
    pub(in super::super) fn read(
        log: &SessionLog,
        m: &Model,
        seq: Seq,
        id: &ToolUseId,
        open: bool,
        opening: Opening,
    ) -> Self {
        // One whose work isn't kept apart is drawn as one whose work was gathered, of which an agent that doesn't
        // record its subagents' work has none.
        if kept_apart(log, m) {
            let record = m.subagents.started_by(id).cloned();
            let calls = record.as_ref().map_or(0, |record| usize::try_from(record.calls).unwrap_or(usize::MAX));
            m.subagent_rows.borrow_mut().insert(id.clone(), seq);
            let working = record.as_ref().filter(|record| record.status == SubagentStatus::Working);
            let lately = working.filter(|_| open).map_or_default(|record| lately::lately(record));
            let lately = lately
                .into_iter()
                .map(|Lately { block, key, result }| {
                    // What came back is in the subagent's events, which the chat doesn't hold, so it's the record's.
                    let mut around = BlockAround::read(log, m, seq, &block, key, opening);
                    if let Some(result) = &result {
                        around.reply = Reply::of(result, m);
                    }
                    (block, around)
                })
                .collect();
            return Self { call: id.clone(), calls, events: None, apart: true, record, lately };
        }
        // A log by `seq` is from a hub too old to keep a subagent's events apart, which names each by its call. One by
        // place holds none of a subagent's.
        let subagent = SubagentId::of_call(id);
        let its_own = |event: &&SessionEvent| parent_of(&event.body) == Some(&subagent);
        let mut gathered = Self {
            call: id.clone(),
            calls: 0,
            events: open.then(Vec::new),
            apart: false,
            record: None,
            lately: Vec::new(),
        };
        for event in nearby::events_after(log, seq, REACH).filter(its_own) {
            let blocks = blocks_of(&event.body);
            gathered.calls += blocks.iter().filter(|block| matches!(block, ContentBlock::ToolUse { .. })).count();
            if let Some(events) = &mut gathered.events {
                events.push((event.clone(), Around::read_as(log, m, event.seq, &event.body, opening)));
            }
        }
        gathered
    }
}

/// Whether `event` is a subagent's, and drawn in the call that started it, so its own row is empty. It is when the
/// call is close enough before it to have gathered it. Otherwise, such as when the call is further back than the log
/// holds, it's drawn where it is, so it shows somewhere.
pub(crate) fn drawn_in_its_call(log: &SessionLog, m: &Model, event: &SessionEvent) -> bool {
    its_call(log, m, event).is_some()
}

/// The call `event` is drawn in, if it's a subagent's drawn in the call that started it, as [`drawn_in_its_call`]
/// says.
pub(crate) fn its_call(log: &SessionLog, m: &Model, event: &SessionEvent) -> Option<Seq> {
    // A log by place holds one agent's or subagent's events, so a call and its subagent's work are never both in it.
    if log.by() == By::Place {
        return None;
    }
    let parent = parent_of(&event.body)?;
    let call = m.subagent_calls.borrow_mut().find(log, event.seq, parent)?;
    // The same reach as `Subagent::read`, which gathers the events after its call's row.
    let (row, call_row) = (log.row_of(event.seq)?, log.row_of(call)?);
    (call_row < row && row - call_row <= REACH).then_some(call)
}

/// Where the calls that started subagents are, as rows have looked for them. A subagent's events, which can fill a
/// screen, each look for theirs on every frame.
#[derive(Default)]
pub(crate) struct SubagentCalls {
    /// Each call found, by the subagent it started. A call never moves, but the log can let go of its page, and then
    /// it's forgotten.
    pub(super) found: HashMap<SubagentId, Seq>,
    /// The events whose call wasn't within reach before them, by seq. The log may have the call once it has more
    /// events, so these are looked at again then.
    pub(super) missed: HashSet<Seq>,
    /// How many times the log had had events added when `missed` was last emptied.
    pub(super) added: u64,
}

impl SubagentCalls {
    /// Where the call that started `subagent` is, among the events within reach before the one at `seq`.
    pub(super) fn find(&mut self, log: &SessionLog, seq: Seq, subagent: &SubagentId) -> Option<Seq> {
        match self.found.get(subagent) {
            Some(&call) if nearby::is_loaded(log, call) => return Some(call),
            // Its page was let go, so it isn't drawn, and its subagent's events have to be drawn where they are.
            Some(_) => {
                self.found.remove(subagent);
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
        // A log by `seq` is from a hub too old to keep a subagent's events apart, which names each subagent by the
        // call that started it.
        let found = nearby::call_before(log, seq, &ToolUseId::from(subagent.as_str()), REACH);
        let found = found.filter(|(_, hands_off)| *hands_off).map(|(call, _)| call);
        match found {
            Some(call) => {
                self.found.insert(subagent.clone(), call);
            }
            None => {
                self.missed.insert(seq);
            }
        }
        found
    }
}

/// The subagent an event is from, if it's from one.
fn parent_of(body: &EventBody) -> Option<&SubagentId> {
    let (EventBody::Assistant { subagent, .. } | EventBody::AgentUser { subagent, .. }) = body else {
        return None;
    };
    subagent.as_ref()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use client_core::session_log::SessionLog;
    use protocol::capability::{Capabilities, Claimed};
    use protocol::event::SessionEvent;
    use protocol::model::{Place, RunCapabilities, RunId, Seq, SubagentId, SubagentStatus, ToolUseId};

    use super::nearby::is_loaded;
    use super::{Opening, Record, Subagent, drawn_in_its_call};
    use crate::fixtures::{calls, calls_to, event, log, record, result, within};
    use crate::transcript::Model;

    #[test]
    fn a_subagent_s_event_out_of_its_call_s_reach_is_drawn_where_it_is() {
        let report = |seq| event(seq, within("agent", calls(&["report"])));
        let log = log([
            event(Seq::new(1), calls_to("Agent", &["agent"])),
            report(Seq::new(2)),
            report(Seq::new(2001)),
            report(Seq::new(2002)),
        ]);
        let m = Model::default();

        assert!(drawn_in_its_call(&log, &m, &report(Seq::new(2))));
        assert!(drawn_in_its_call(&log, &m, &report(Seq::new(2001))), "the last event the call gathers");
        assert!(!drawn_in_its_call(&log, &m, &report(Seq::new(2002))), "too far for the call to gather");
        let orphan = event(Seq::new(3), within("a call that isn't here", calls(&["x"])));
        assert!(!drawn_in_its_call(&log, &m, &orphan), "with no call to draw it");
        let gathered = Subagent::read(&log, &m, Seq::new(1), &"agent".into(), true, Opening::AsChosen).events.unwrap();
        let seqs: Vec<u64> = gathered.iter().map(|(event, _)| event.seq.get()).collect();
        assert_eq!(seqs, [2, 2001], "the call gathers exactly what's drawn in it");
    }

    #[test]
    fn a_call_not_found_is_looked_for_again_once_the_log_has_more() {
        let work = event(Seq::new(5), within("agent", calls(&["a"])));
        let mut log = log([work.clone()]);
        let m = Model::default();
        assert!(!drawn_in_its_call(&log, &m, &work), "its call isn't loaded");

        log.apply_event(event(Seq::new(1), calls_to("Agent", &["agent"])));

        assert!(drawn_in_its_call(&log, &m, &work));
    }

    #[test]
    fn a_call_the_log_lets_go_of_no_longer_draws_its_subagent_s_work() {
        let work = |seq| event(seq, within("agent", calls(&["work"])));
        let mut log = log((1..=1500).map(|seq| event(Seq::new(seq), result("x"))));
        log.apply_event(event(Seq::new(1501), calls_to("Agent", &["agent"])));
        log.apply_event(work(Seq::new(1502)));
        let m = Model::default();
        assert!(drawn_in_its_call(&log, &m, &work(Seq::new(1502))), "found, and remembered");

        for seq in 1503..=2500 {
            log.apply_event(event(Seq::new(seq), result("x")));
        }
        log.apply_event(work(Seq::new(2501)));

        assert!(!is_loaded(&log, Seq::new(1501)), "the log has let go of the call's page");
        assert!(!drawn_in_its_call(&log, &m, &work(Seq::new(2501))), "drawn where it is, since its call isn't");
    }

    #[test]
    fn only_a_call_that_hands_off_draws_a_subagent_s_work() {
        let work = event(Seq::new(2), within("not-an-agent", calls(&["a"])));
        let log = log([event(Seq::new(1), calls(&["not-an-agent"])), work.clone()]);

        assert!(!drawn_in_its_call(&log, &Model::default(), &work));
    }

    /// A hub that keeps a subagent's work apart names it as its agent does, which needn't be its call, so its call's
    /// row finds its record by the call the record keeps, and is measured again by that call when the record changes.
    #[test]
    fn a_call_s_row_finds_the_record_of_the_subagent_it_started_whatever_its_agent_named_it() {
        let mut log = SessionLog::by_place(Place::default());
        log.apply_event(SessionEvent { place: Place::new(1), ..event(Seq::new(1), calls_to("Agent", &["agent"])) });
        let mut m = Model::default();
        let started = Record { id: Some("thread-1".into()), calls: 3, ..record("agent", SubagentStatus::Working) };
        m.subagents.insert(Arc::new(started));

        let row = Subagent::read(&log, &m, Seq::new(1), &"agent".into(), false, Opening::AsChosen);

        assert_eq!(row.record.as_deref().map(Record::id), Some("thread-1".into()));
        assert_eq!(row.calls, 3);
        assert_eq!(m.subagent_rows.borrow().get(&ToolUseId::from("agent")), Some(&Seq::new(1)));
        assert_eq!(m.subagents.subagent_started_by(&"agent".into()), SubagentId::from("thread-1"), "its tab's");
        let unrecorded = m.subagents.subagent_started_by(&"other".into());
        assert_eq!(unrecorded, SubagentId::from("other"), "one with no record is named by its call");
    }

    /// An agent that doesn't record its subagents' work, as Antigravity's doesn't, has none apart for the tab, so its
    /// call's row has no record or button, as one whose work wasn't kept apart.
    #[test]
    fn a_call_s_row_keeps_no_work_apart_for_an_agent_that_doesn_t_record_its_subagents() {
        let mut log = SessionLog::by_place(Place::default());
        log.apply_event(SessionEvent { place: Place::new(1), ..event(Seq::new(1), calls_to("Agent", &["agent"])) });
        let mut m = Model::default();
        let read = |m: &Model| Subagent::read(&log, m, Seq::new(1), &"agent".into(), false, Opening::AsChosen).apart;
        assert!(read(&m), "until its claims are known");

        let mut claims = Capabilities::default();
        m.claims = Some(RunCapabilities::new(RunId::new(1), claims.clone()));
        assert!(!read(&m), "without subagents");
        claims.background.subagents = Some(Claimed {});
        m.claims = Some(RunCapabilities::new(RunId::new(1), claims));
        assert!(read(&m), "with them");
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
        let log = log((1..).zip(events).map(|(seq, body)| event(Seq::new(seq), body)));
        let m = Model::default();

        let open = Subagent::read(&log, &m, Seq::new(1), &"agent".into(), true, Opening::AsChosen);
        let closed = Subagent::read(&log, &m, Seq::new(1), &"agent".into(), false, Opening::AsChosen);

        assert_eq!(open.calls, 2);
        let seqs: Vec<u64> = open.events.unwrap().iter().map(|(event, _)| event.seq.get()).collect();
        assert_eq!(seqs, [2, 5], "only its own events, in order");
        assert_eq!(closed.calls, 2);
        assert!(closed.events.is_none(), "a closed call only counts");
    }
}
