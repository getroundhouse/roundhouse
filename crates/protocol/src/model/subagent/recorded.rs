//! Each subagent's record, made from its chat's events as the hub keeps it as it records them. The hub changes its
//! records with SQL of its own, in the transaction that records each event, and its store's tests hold that to this, on
//! the same events. A fake hub makes its records by this, so they agree with its events as a hub's would. Only they
//! use it, so it's behind `testing`.

use super::record::{Subagent, SubagentStatus};
use crate::event::{ContentBlock, EventBody, SessionEvent};
use crate::model::{ModelName, Rev, SubagentId};

impl Subagent {
    /// The records of the subagents in `log`, oldest first, each by when it started, or last started again, then by
    /// which was recorded first, each with the `seq` of the event that last changed it as its revision. A subagent's
    /// record starts with the agent's word that it started, ends with its word that it ended, or with the agent, which
    /// takes its subagents with it, works again if the agent says it started again, and counts the calls in between
    /// from its own events, which also say what it's done lately, and its replies the model it runs on. How many tokens
    /// its conversation holds is the agent's word too, as it works and as it ends.
    pub fn recorded_from(log: &[SessionEvent]) -> Vec<Subagent> {
        let mut held = Vec::new();
        for event in log {
            for subagent in note(&mut held, event) {
                subagent.rev = Rev::new(event.seq.get());
            }
        }
        held.sort_by_key(|subagent| subagent.started_at_ms);
        held
    }
}

/// Changes the records `event` says something about, and gives back those it changed.
fn note<'a>(held: &'a mut Vec<Subagent>, event: &SessionEvent) -> Vec<&'a mut Subagent> {
    let at = event.ts_ms;
    match &event.body {
        EventBody::SubagentStarted { .. } => started(held, event).into_iter().collect(),
        // Only while it works, since its end has the last word, and only a count that differs.
        EventBody::SubagentContext { subagent, tokens } => {
            let Some(it) = working(held, subagent).filter(|it| it.context_tokens != Some(*tokens)) else {
                return Vec::new();
            };
            it.context_tokens = Some(*tokens);
            vec![it]
        }
        // What it's done lately is only kept while it works. An end that counts no tokens leaves the count it had.
        EventBody::SubagentEnded { subagent, outcome, report, context_tokens } => {
            let Some(it) = working(held, subagent) else { return Vec::new() };
            (it.status, it.ended_at_ms, it.lately) = (SubagentStatus::ended(*outcome), Some(at), None);
            it.report.clone_from(report);
            it.context_tokens = context_tokens.or(it.context_tokens);
            vec![it]
        }
        // A subagent goes with its agent.
        EventBody::AgentExited { .. } => {
            let mut stopped = Vec::new();
            for it in held.iter_mut().filter(|it| it.status == SubagentStatus::Working) {
                (it.status, it.ended_at_ms, it.lately) = (SubagentStatus::Stopped, Some(at), None);
                stopped.push(it);
            }
            stopped
        }
        // Its calls and what it wrote come from its side, and what came back for its calls from the user's.
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => match &event.subagent {
            Some(subagent) => worked(held, subagent, &event.body, blocks).into_iter().collect(),
            None => Vec::new(),
        },
        EventBody::UserPrompt { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::OwnTurn(_)
        | EventBody::AgentLimits { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::RequestFailed { .. }
        | EventBody::AgentMode { .. }
        | EventBody::TaskChanged { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::ConversationStarted { .. }
        | EventBody::VoiceCut { .. }
        | EventBody::Unknown => Vec::new(),
    }
}

/// The record a start makes, or the one it starts again. One that starts again after it ended, as Claude's does when
/// it's messaged, works again, under the call that first started it, with what it was first asked, from when it started
/// again, so how long it's at it isn't the time it sat finished, and its report is the one it gives next. Another call
/// can't start one that's already a subagent's.
fn started<'a>(held: &'a mut Vec<Subagent>, event: &SessionEvent) -> Option<&'a mut Subagent> {
    let EventBody::SubagentStarted { call, kind, description, prompt, in_background, .. } = &event.body else {
        return None;
    };
    let id = event.body.subagent_started()?;
    if let Some(again) = held.iter().position(|it| it.id() == id) {
        let again = &mut held[again];
        if again.status == SubagentStatus::Working {
            return None;
        }
        (again.status, again.in_background, again.started_at_ms) =
            (SubagentStatus::Working, *in_background, event.ts_ms);
        (again.ended_at_ms, again.report) = (None, None);
        return Some(again);
    }
    if held.iter().any(|it| it.call == *call) {
        return None;
    }
    held.push(Subagent {
        session_id: event.session_id.clone(),
        id: Some(id),
        call: call.clone(),
        kind: kind.clone(),
        description: description.clone(),
        prompt: prompt.clone(),
        in_background: *in_background,
        model: None,
        status: SubagentStatus::Working,
        started_at_ms: event.ts_ms,
        ended_at_ms: None,
        context_tokens: None,
        calls: 0,
        lately: None,
        report: None,
        rev: Rev::default(),
    });
    held.last_mut()
}

/// What one of `subagent`'s own messages, `body`, with `blocks`, adds to its record, if it changes it: the calls it
/// made, what it's done lately, and from a reply, the model it runs on, which most of its replies name as the last did.
fn worked<'a>(
    held: &'a mut [Subagent],
    subagent: &SubagentId,
    body: &EventBody,
    blocks: &[ContentBlock],
) -> Option<&'a mut Subagent> {
    let it = held.iter_mut().find(|it| it.id() == *subagent)?;
    let mut changed = false;
    if let EventBody::Assistant { model: Some(model), .. } = body
        && let Ok(model) = ModelName::try_from(model.clone())
        && it.model.as_ref() != Some(&model)
    {
        it.model = Some(model);
        changed = true;
    }
    let made = blocks.iter().filter(|block| matches!(block, ContentBlock::Call { .. })).count();
    it.calls = it.calls.saturating_add(u32::try_from(made).unwrap_or(u32::MAX));
    changed |= made > 0;
    if it.status == SubagentStatus::Working {
        let mut lately = it.lately.clone().unwrap_or_default();
        if lately.add(blocks, matches!(body, EventBody::Assistant { .. })) {
            it.lately = Some(lately);
            changed = true;
        }
    }
    changed.then_some(it)
}

/// The record of `subagent`, if it's working.
fn working<'a>(held: &'a mut [Subagent], subagent: &SubagentId) -> Option<&'a mut Subagent> {
    held.iter_mut().find(|it| it.id() == *subagent && it.status == SubagentStatus::Working)
}
