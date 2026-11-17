//! Each subagent's record, made from its chat's events as the hub keeps it as it records them, so a world's records
//! and events always agree. Its model is the one its newest reply came from, and its tokens the agent's newest count
//! of what its conversation holds.

use std::collections::BTreeMap;

use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Lately, LatelyCall, LatelyResult, ModelName, Rev, Said, Subagent, SubagentId, SubagentStatus};

/// The records of the subagents in `log`, by subagent.
#[expect(clippy::too_many_lines, reason = "a short arm for each event that changes a record, and the rest by name")]
pub(super) fn records_of(log: &[SessionEvent]) -> Vec<Subagent> {
    let mut held: BTreeMap<SubagentId, Subagent> = BTreeMap::new();
    for event in log {
        let rev = Rev::new(event.seq.get());
        match &event.body {
            EventBody::SubagentStarted { call, kind, description, prompt, in_background, .. } => {
                let Some(id) = event.body.subagent_started() else { continue };
                held.entry(id.clone()).or_insert(Subagent {
                    id: Some(id),
                    session_id: event.session_id.clone(),
                    kind: kind.clone(),
                    description: description.clone(),
                    prompt: prompt.clone(),
                    in_background: *in_background,
                    started_at_ms: event.ts_ms,
                    rev,
                    ..protocol::fixtures::subagent(call.as_str())
                });
            }
            EventBody::SubagentContext { subagent, tokens } => {
                let working = held.get_mut(subagent).filter(|s| s.status == SubagentStatus::Working);
                if let Some(subagent) = working.filter(|s| s.context_tokens != Some(*tokens)) {
                    subagent.context_tokens = Some(*tokens);
                    subagent.rev = rev;
                }
            }
            EventBody::SubagentEnded { subagent, outcome, report, context_tokens } => {
                if let Some(subagent) = held.get_mut(subagent).filter(|s| s.status == SubagentStatus::Working) {
                    subagent.status = SubagentStatus::ended(*outcome);
                    subagent.report.clone_from(report);
                    subagent.context_tokens = context_tokens.or(subagent.context_tokens);
                    subagent.lately = None;
                    subagent.ended_at_ms = Some(event.ts_ms);
                    subagent.rev = rev;
                }
            }
            EventBody::Assistant { blocks, subagent: Some(parent), .. }
            | EventBody::AgentUser { blocks, subagent: Some(parent), .. } => {
                let Some(subagent) = held.get_mut(parent) else { continue };
                if let EventBody::Assistant { model: Some(model), .. } = &event.body
                    && let Ok(model) = ModelName::try_from(model.clone())
                {
                    subagent.model = Some(model);
                }
                for block in blocks {
                    note(subagent, block);
                }
                subagent.rev = rev;
            }
            // A subagent goes with its agent.
            EventBody::AgentExited { .. } => {
                for subagent in held.values_mut().filter(|s| s.status == SubagentStatus::Working) {
                    subagent.status = SubagentStatus::Stopped;
                    subagent.lately = None;
                    subagent.ended_at_ms = Some(event.ts_ms);
                    subagent.rev = rev;
                }
            }
            // Nothing of a subagent's record, an agent's own message included.
            EventBody::UserPrompt { .. }
            | EventBody::PromptTaken { .. }
            | EventBody::PromptDropped { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentTasks { .. }
            | EventBody::TaskChanged { .. }
            | EventBody::OwnTurn(_)
            | EventBody::AgentLimits { .. }
            | EventBody::Assistant { .. }
            | EventBody::AgentUser { .. }
            | EventBody::ToolCallDropped { .. }
            | EventBody::TurnEnded { .. }
            | EventBody::RequestFailed { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::Notice { .. }
            | EventBody::AgentMode { .. }
            | EventBody::AgentOther { .. }
            | EventBody::AgentCompacting { .. }
            | EventBody::CompactionAsked { .. }
            | EventBody::AgentCompacted { .. }
            | EventBody::CacheExpired { .. }
            | EventBody::Unknown => {}
        }
    }
    // The hub keeps what it's done lately once there's something to say of it.
    let kept =
        |subagent: Subagent| Subagent { lately: subagent.lately.filter(|work| *work != Lately::default()), ..subagent };
    held.into_values().map(kept).collect()
}

/// What one of a subagent's blocks adds to its record: its calls, and while it works, what it's done lately, as the hub
/// keeps them, but for cutting a long text short, which no world's texts are long enough to need.
fn note(subagent: &mut Subagent, block: &ContentBlock) {
    if let ContentBlock::Call { .. } = block {
        subagent.calls += 1;
    }
    if subagent.status == SubagentStatus::Working {
        note_lately(subagent.lately.get_or_insert_default(), block);
    }
}

/// What one of a subagent's blocks adds to what it's done lately.
fn note_lately(work: &mut Lately, block: &ContentBlock) {
    match block {
        ContentBlock::Text { text, .. } => {
            let text = match work.since.take() {
                Some(since) => format!("{}\n\n{text}", since.text),
                None => text.clone(),
            };
            work.since = Some(Said { text, cut: false });
        }
        ContentBlock::Call { id, call } => {
            work.calls.push(LatelyCall { before: work.since.take(), id: id.clone(), call: call.clone(), result: None });
            if work.calls.len() > 3 {
                work.calls.remove(0);
            }
        }
        ContentBlock::CallResult { tool_use_id, output, is_error, denied, images } => {
            if let Some(made) = work.calls.iter_mut().find(|made| made.id == *tool_use_id) {
                let (output, is_error, images, denied) = (output.clone(), *is_error, images.clone(), denied.clone());
                made.result = Some(LatelyResult { output, is_error, cut: false, images, denied });
            }
        }
        ContentBlock::Thinking { .. } | ContentBlock::Image { .. } | ContentBlock::Unknown => {}
    }
}
