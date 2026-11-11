//! Each subagent's record, made from its chat's events as the hub keeps it as it records them, so a screen's records
//! and events always agree.

#![cfg(test)]

use std::collections::BTreeMap;

use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Call, RecentWork, Rev, Subagent, SubagentStatus, ToolUseId};

/// The records of the subagents in `log`, by the call that started each.
pub(crate) fn records_of(log: &[SessionEvent]) -> Vec<Subagent> {
    let mut held: BTreeMap<ToolUseId, Subagent> = BTreeMap::new();
    for event in log {
        let rev = Rev::new(event.seq);
        match &event.body {
            EventBody::SubagentStarted { call, kind, description, prompt, in_background } => {
                held.entry(call.clone()).or_insert(Subagent {
                    session_id: event.session_id.clone(),
                    call: call.clone(),
                    kind: kind.clone(),
                    description: description.clone(),
                    prompt: prompt.clone(),
                    in_background: *in_background,
                    status: SubagentStatus::Working,
                    started_at_ms: event.ts_ms,
                    ended_at_ms: None,
                    calls: 0,
                    latest_call: None,
                    recent: RecentWork::default(),
                    report: None,
                    rev,
                });
            }
            EventBody::SubagentEnded { call, outcome, report } => {
                if let Some(subagent) = held.get_mut(call).filter(|s| s.status == SubagentStatus::Working) {
                    subagent.status = SubagentStatus::ended(*outcome);
                    subagent.report.clone_from(report);
                    subagent.ended_at_ms = Some(event.ts_ms);
                    subagent.rev = rev;
                }
            }
            EventBody::Assistant { blocks, parent_tool_use_id: Some(parent), .. } => {
                let Some(subagent) = held.get_mut(parent) else { continue };
                for block in blocks {
                    if let ContentBlock::ToolUse { id, name, input } = block {
                        subagent.calls += 1;
                        let call = Call { id: id.as_str().into(), name: name.clone(), input: input.clone() };
                        subagent.latest_call = Some(call);
                        subagent.rev = rev;
                    }
                }
            }
            // A subagent goes with its agent.
            EventBody::AgentExited { .. } => {
                for subagent in held.values_mut().filter(|s| s.status == SubagentStatus::Working) {
                    subagent.status = SubagentStatus::Stopped;
                    subagent.ended_at_ms = Some(event.ts_ms);
                    subagent.rev = rev;
                }
            }
            _ => {}
        }
    }
    held.into_values().collect()
}
