//! Each subagent's record, made from its chat's events as the hub keeps it as it records them, so a screen's records
//! and events always agree.

#![cfg(test)]

use std::collections::BTreeMap;

use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Call, CallResult, RecentCall, RecentWork, Rev, Said, Subagent, SubagentStatus, ToolUseId};

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
                    subagent.recent = RecentWork::default();
                    subagent.ended_at_ms = Some(event.ts_ms);
                    subagent.rev = rev;
                }
            }
            EventBody::Assistant { blocks, parent_tool_use_id: Some(parent), .. }
            | EventBody::AgentUser { blocks, parent_tool_use_id: Some(parent), .. } => {
                let Some(subagent) = held.get_mut(parent) else { continue };
                for block in blocks {
                    note(subagent, block);
                }
                subagent.rev = rev;
            }
            // A subagent goes with its agent.
            EventBody::AgentExited { .. } => {
                for subagent in held.values_mut().filter(|s| s.status == SubagentStatus::Working) {
                    subagent.status = SubagentStatus::Stopped;
                    subagent.recent = RecentWork::default();
                    subagent.ended_at_ms = Some(event.ts_ms);
                    subagent.rev = rev;
                }
            }
            _ => {}
        }
    }
    held.into_values().collect()
}

/// What one of a subagent's blocks adds to its record: its calls, and while it works, what it's done lately, as the hub
/// keeps them, but for cutting a long text short, which the screens' texts never are.
fn note(subagent: &mut Subagent, block: &ContentBlock) {
    if let ContentBlock::ToolUse { id, name, input } = block {
        subagent.calls += 1;
        subagent.latest_call = Some(Call { id: id.as_str().into(), name: name.clone(), input: input.clone() });
    }
    if subagent.status != SubagentStatus::Working {
        return;
    }
    let work = &mut subagent.recent;
    match block {
        ContentBlock::Text { text, .. } => {
            let text = match work.since.take() {
                Some(since) => format!("{}\n\n{text}", since.text),
                None => text.clone(),
            };
            work.since = Some(Said { text, cut: false });
        }
        ContentBlock::ToolUse { id, name, input } => {
            let call = Call { id: id.as_str().into(), name: name.clone(), input: input.clone() };
            work.calls.push(RecentCall { before: work.since.take(), call, result: None });
            if work.calls.len() > 3 {
                work.calls.remove(0);
            }
        }
        ContentBlock::ToolResult { tool_use_id, is_error, text, images, encrypted, .. } => {
            if let Some(made) = work.calls.iter_mut().find(|made| made.call.id == *tool_use_id) {
                let (is_error, text, images, encrypted) = (*is_error, text.clone(), images.clone(), *encrypted);
                made.result = Some(CallResult { is_error, text, cut: false, images, encrypted });
            }
        }
        ContentBlock::Thinking { .. } | ContentBlock::Image { .. } | ContentBlock::Unknown => {}
    }
}
