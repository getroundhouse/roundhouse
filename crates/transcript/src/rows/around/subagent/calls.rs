use super::super::{BlockAround, Opening, Reply};
use super::lately;
use crate::transcript::Model;
use client_core::session_log::SessionLog;
use lately::Lately;
use protocol::event::ContentBlock;
use protocol::model::{Seq, Subagent as Record, SubagentStatus, ToolUseId};
use std::sync::Arc;

/// What a subagent has done so far.
pub(in crate::rows) struct Subagent {
    /// The call that started it.
    pub(in crate::rows) call: ToolUseId,
    /// How many calls it's made.
    pub(in crate::rows) calls: usize,
    /// Its work is kept apart from the chat's, in the subagent tab, so it's drawn from its record: for an agent that
    /// records its subagents' work. Antigravity's adapter can't, since its CLI doesn't send it, so its subagents have
    /// none kept apart for the tab to show.
    pub(in crate::rows) apart: bool,
    /// Its record, once the hub has sent it.
    pub(in crate::rows) record: Option<Arc<Record>>,
    /// From its record, while it works and its call is open: what it's done lately, as blocks, oldest first, each
    /// with what drawing it needs.
    pub(in crate::rows) lately: Vec<(ContentBlock, BlockAround)>,
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
        // An agent that doesn't record its subagents' work has nothing of it to draw.
        if !m.records_subagents() {
            return Self { call: id.clone(), calls: 0, apart: false, record: None, lately: Vec::new() };
        }
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
        Self { call: id.clone(), calls, apart: true, record, lately }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use protocol::capability::{Capabilities, Claimed};
    use protocol::model::{RunCapabilities, RunId, Seq, SubagentId, SubagentStatus, ToolUseId};

    use super::{Opening, Record, Subagent};
    use crate::fixtures::{event, log, record, subagents};
    use crate::transcript::Model;

    /// The hub names a subagent as its agent does, which needn't be its call, so its call's row finds its record by the
    /// call the record keeps, and is measured again by that call when the record changes.
    #[test]
    fn a_call_s_row_finds_the_record_of_the_subagent_it_started_whatever_its_agent_named_it() {
        let log = log([event(Seq::new(1), subagents(&["agent"]))]);
        let mut m = Model::default();
        let started = Record { id: "thread-1".into(), calls: 3, ..record("agent", SubagentStatus::Working) };
        m.subagents.insert(Arc::new(started));

        let row = Subagent::read(&log, &m, Seq::new(1), &"agent".into(), false, Opening::AsChosen);

        assert_eq!(row.record.as_deref().map(|record| record.id.clone()), Some("thread-1".into()));
        assert_eq!(row.calls, 3);
        assert_eq!(m.subagent_rows.borrow().get(&ToolUseId::from("agent")), Some(&Seq::new(1)));
        assert_eq!(m.subagents.subagent_started_by(&"agent".into()), SubagentId::from("thread-1"), "its tab's");
        let unrecorded = m.subagents.subagent_started_by(&"other".into());
        assert_eq!(unrecorded, SubagentId::from("other"), "one with no record is named by its call");
    }

    /// An agent that doesn't record its subagents' work, as Antigravity's doesn't, has none apart for the tab, so its
    /// call's row has no record or button.
    #[test]
    fn a_call_s_row_keeps_no_work_apart_for_an_agent_that_doesn_t_record_its_subagents() {
        let log = log([event(Seq::new(1), subagents(&["agent"]))]);
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
}
