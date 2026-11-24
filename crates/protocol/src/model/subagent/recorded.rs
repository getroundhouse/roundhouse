//! Each subagent's record, made from its chat's events. The hub keeps its records by this, in the transaction that
//! records each event, over only those [`Subagent::touched_by`] says the event can change. A fake hub makes the records
//! of a whole log by it, so they agree with its events as a hub's would.

use super::record::{Subagent, SubagentStatus};
use crate::event::{ContentBlock, EventBody, SessionEvent, TaskOutcome};
use crate::model::{ModelName, Rev, SubagentId, ToolUseId};

/// The records of a chat's subagents that an event can change, as [`Subagent::touched_by`] says.
#[derive(Clone, Debug, PartialEq)]
pub enum TouchedSubagents {
    /// The one with this ID.
    One(SubagentId),
    /// The one with this ID, which starts again if it ended, and the one the call started, since a call starts only
    /// one.
    Starting { subagent: SubagentId, call: ToolUseId },
    /// Every one still working, since an agent that exits takes them with it.
    Working,
}

impl Subagent {
    /// The records of the subagents in `log`, oldest first, each by when it started, or last started again, then by
    /// which was recorded first, each with the `seq` of the event that last changed it as its revision, as
    /// [`Subagent::note`] says each event changes them. Only a fake hub uses it, so it's behind `testing`.
    #[cfg(any(test, feature = "testing"))]
    pub fn recorded_from(log: &[SessionEvent]) -> Vec<Subagent> {
        let mut held = Vec::new();
        for event in log {
            for subagent in Subagent::note(&mut held, event) {
                subagent.rev = Rev::new(event.seq.get());
            }
        }
        held.sort_by_key(|subagent| subagent.started_at_ms);
        held
    }

    /// Which records `event` can change, which are all of its chat's that [`Subagent::note`] reads for it, or `None`
    /// for an event that says nothing of a subagent, as most don't.
    pub fn touched_by(event: &SessionEvent) -> Option<TouchedSubagents> {
        Some(match saying(event)? {
            Saying::Started(start) => TouchedSubagents::Starting { subagent: start.subagent, call: start.call.clone() },
            Saying::Context { subagent, .. } | Saying::Ended { subagent, .. } | Saying::Worked { subagent, .. } => {
                TouchedSubagents::One(subagent.clone())
            }
            Saying::Exited => TouchedSubagents::Working,
        })
    }

    /// Changes the records in `held` that `event` says something about, and gives back those it changed, and only
    /// those, so one it leaves as it was keeps its revision. `held` holds at least the records of the chat that
    /// [`Subagent::touched_by`] says `event` can change. A start may add one, which has no revision yet.
    ///
    /// A subagent's record starts with the agent's word that it started, ends with its word that it ended, or with the
    /// agent, which takes its subagents with it, works again if the agent says it started again, and counts the calls in
    /// between from its own events, which also say what it's done lately, and its replies the model it runs on. How
    /// many tokens its conversation holds is the agent's word too, as it works and as it ends.
    pub fn note<'a>(held: &'a mut Vec<Subagent>, event: &SessionEvent) -> Vec<&'a mut Subagent> {
        let at = event.ts_ms;
        let Some(saying) = saying(event) else { return Vec::new() };
        match saying {
            Saying::Started(start) => started(held, event, start).into_iter().collect(),
            // Only while it works, since its end has the last word, and only a count that differs.
            Saying::Context { subagent, tokens } => {
                let Some(it) = working(held, subagent).filter(|it| it.context_tokens != Some(tokens)) else {
                    return Vec::new();
                };
                it.context_tokens = Some(tokens);
                vec![it]
            }
            // What it's done lately is only kept while it works. An end that counts no tokens leaves the count it had.
            Saying::Ended { subagent, outcome, report, context_tokens } => {
                let Some(it) = working(held, subagent) else { return Vec::new() };
                (it.status, it.ended_at_ms, it.lately) = (SubagentStatus::ended(outcome), Some(at), None);
                it.report.clone_from(report);
                it.context_tokens = context_tokens.or(it.context_tokens);
                vec![it]
            }
            // A subagent goes with its agent.
            Saying::Exited => {
                let mut stopped = Vec::new();
                for it in held.iter_mut().filter(|it| it.status == SubagentStatus::Working) {
                    (it.status, it.ended_at_ms, it.lately) = (SubagentStatus::Stopped, Some(at), None);
                    stopped.push(it);
                }
                stopped
            }
            Saying::Worked { subagent, blocks, model, wrote } => {
                worked(held, subagent, blocks, model, wrote).into_iter().collect()
            }
        }
    }
}

/// What an event says of its chat's subagents.
enum Saying<'a> {
    Started(Start<'a>),
    Context {
        subagent: &'a SubagentId,
        tokens: u64,
    },
    Ended {
        subagent: &'a SubagentId,
        outcome: TaskOutcome,
        report: &'a Option<String>,
        context_tokens: Option<u64>,
    },
    Exited,
    /// One of the subagent's own messages, with `blocks`. `wrote` says it's from the subagent's side, so its text is
    /// what it wrote, and `model` names the model a reply came from.
    Worked {
        subagent: &'a SubagentId,
        blocks: &'a [ContentBlock],
        model: Option<&'a String>,
        wrote: bool,
    },
}

/// What a start says of the subagent it starts.
struct Start<'a> {
    subagent: SubagentId,
    call: &'a ToolUseId,
    kind: &'a Option<String>,
    description: &'a Option<String>,
    prompt: &'a Option<String>,
    in_background: bool,
}

/// What `event` says of its chat's subagents, if anything.
fn saying(event: &SessionEvent) -> Option<Saying<'_>> {
    match &event.body {
        EventBody::SubagentStarted { call, kind, description, prompt, in_background, .. } => {
            let subagent = event.body.subagent_started()?;
            let in_background = *in_background;
            Some(Saying::Started(Start { subagent, call, kind, description, prompt, in_background }))
        }
        EventBody::SubagentContext { subagent, tokens } => Some(Saying::Context { subagent, tokens: *tokens }),
        EventBody::SubagentEnded { subagent, outcome, report, context_tokens } => {
            Some(Saying::Ended { subagent, outcome: *outcome, report, context_tokens: *context_tokens })
        }
        EventBody::AgentExited { .. } => Some(Saying::Exited),
        // Its calls and what it wrote come from its side, and what came back for its calls from the user's.
        EventBody::Assistant { blocks, model, .. } => {
            Some(Saying::Worked { subagent: event.subagent.as_ref()?, blocks, model: model.as_ref(), wrote: true })
        }
        EventBody::AgentUser { blocks, .. } => {
            Some(Saying::Worked { subagent: event.subagent.as_ref()?, blocks, model: None, wrote: false })
        }
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
        | EventBody::Unknown => None,
    }
}

/// The record `start` makes, or the one it starts again. One that starts again after it ended, as Claude's
/// does when it's messaged, works again, under the call that first started it, with what it was first asked, from when
/// it started again, so how long it's at it isn't the time it sat finished, and its report is the one it gives next.
/// Another call can't start one that's already a subagent's.
fn started<'a>(held: &'a mut Vec<Subagent>, event: &SessionEvent, start: Start<'_>) -> Option<&'a mut Subagent> {
    let Start { subagent, call, kind, description, prompt, in_background } = start;
    if let Some(again) = held.iter().position(|it| it.id == subagent) {
        let again = &mut held[again];
        if again.status == SubagentStatus::Working {
            return None;
        }
        (again.status, again.in_background, again.started_at_ms) =
            (SubagentStatus::Working, in_background, event.ts_ms);
        (again.ended_at_ms, again.report) = (None, None);
        return Some(again);
    }
    if held.iter().any(|it| it.call == *call) {
        return None;
    }
    held.push(Subagent {
        session_id: event.session_id.clone(),
        id: subagent,
        call: call.clone(),
        kind: kind.clone(),
        description: description.clone(),
        prompt: prompt.clone(),
        in_background,
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

/// What one of `subagent`'s own messages, with `blocks`, adds to its record, if it changes it: the calls it made, what
/// it's done lately, and from a reply naming `model`, the model it runs on, which most of its replies name as the last
/// did.
fn worked<'a>(
    held: &'a mut [Subagent],
    subagent: &SubagentId,
    blocks: &[ContentBlock],
    model: Option<&String>,
    wrote: bool,
) -> Option<&'a mut Subagent> {
    let it = held.iter_mut().find(|it| it.id == *subagent)?;
    let mut changed = false;
    if let Some(model) = model
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
        if lately.add(blocks, wrote) {
            it.lately = Some(lately);
            changed = true;
        }
    }
    changed.then_some(it)
}

/// The record of `subagent`, if it's working.
fn working<'a>(held: &'a mut [Subagent], subagent: &SubagentId) -> Option<&'a mut Subagent> {
    held.iter_mut().find(|it| it.id == *subagent && it.status == SubagentStatus::Working)
}
