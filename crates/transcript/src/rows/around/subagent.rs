//! What a subagent has done, for the call that started it, and where each of its events is drawn. Its events arrive
//! after the call, mixed with the agent's own and with other subagents', and [`crate::placement`] draws each in a row
//! of its own straight after the call, under the call's rule while the call is open.

use client_core::session_log::SessionLog;
use protocol::model::{Seq, ToolUseId};

use super::{NEAR, answer_in, nearby};
use crate::calls::agent;
use crate::item_key::ItemKey;
use crate::placement::LogRow;
use crate::transcript::Model;

/// What a subagent has done so far, for its call's line.
pub(in crate::rows) struct Subagent {
    /// How many calls it's made.
    pub(in crate::rows) calls: usize,
    /// Whether any of its work is drawn after the call, which then ends the call's rule rather than the call itself.
    pub(in crate::rows) follows: bool,
}

impl Subagent {
    /// What the subagent started by the call `id` has done.
    pub(super) fn read(m: &Model, id: &str) -> Self {
        match m.placement.call(&ToolUseId::from(id)) {
            Some(call) => Self { calls: call.work.calls, follows: call.work.last.is_some() },
            None => Self { calls: 0, follows: false },
        }
    }
}

/// A call one of a subagent's events is drawn under: the call that started the subagent, or, for a subagent another
/// started, that one's call too. Its work is only drawn while it's open.
pub(in crate::rows) struct Under {
    /// The call's block, whose text keys its report.
    pub(in crate::rows) key: ItemKey,
    /// The event the call is in.
    pub(in crate::rows) seq: Seq,
    /// Whether this event is the last drawn under the call, so the call's rule ends with it.
    pub(in crate::rows) ends: bool,
    /// For the last: what the subagent reported, when it wrote nothing itself and the call's result is its report.
    pub(in crate::rows) report: Option<String>,
}

/// The calls the subagent's event in `row`, which the call `id` started, is drawn under, outermost first.
pub(in crate::rows) fn under(log: &SessionLog, m: &Model, row: LogRow, id: &ToolUseId) -> Vec<Under> {
    let mut under = Vec::new();
    let mut next = Some(id);
    while let Some(id) = next
        && let Some(call) = m.placement.call(id)
    {
        let ends = call.work.last == Some(row);
        // A subagent run in the foreground reports in its call's result. Its own words say the same, when they came.
        let report = (ends && !call.work.said)
            .then(|| nearby::events_after(log, call.seq, NEAR).find_map(|event| answer_in(event, id.as_str())))
            .flatten()
            .and_then(|answer| answer.success().filter(|text| !agent::only_launched(text)).map(str::to_string));
        under.push(Under { key: call.key.clone(), seq: call.seq, ends, report });
        next = m.placement.within(call.row);
    }
    under.reverse();
    under
}
