//! The capabilities each event needs its run to claim.

use crate::capability::{Capability, approvals, asking, background, context, output, turns};
use crate::event::{Asking, CompactedBy, CompactionOutcome, ContentBlock, EventBody, TaskKind};

/// The capabilities an event needs its run to claim. Most need none, since every agent says them.
pub fn needs(body: &EventBody) -> Vec<Capability> {
    match body {
        EventBody::AgentTasks { tasks, .. } => tasks.iter().map(|task| task_needs(task.kind)).collect(),
        EventBody::AgentLimits { .. } => vec![crate::capability::account::Capability::Limits.into()],
        EventBody::SubagentStarted { .. } | EventBody::SubagentContext { .. } | EventBody::SubagentEnded { .. } => {
            vec![background::Capability::Subagents.into()]
        }
        EventBody::Assistant { blocks, .. } => {
            let thinks = blocks.iter().any(|block| matches!(block, ContentBlock::Thinking { .. }));
            thinks.then(|| output::Capability::Thinking.into()).into_iter().collect()
        }
        EventBody::TurnEnded { tokens, cost_usd, only_compacted, .. } => [
            tokens.is_some().then_some(Capability::from(turns::Capability::Tokens)),
            cost_usd.is_some().then_some(turns::Capability::Cost.into()),
            only_compacted.then_some(context::Capability::Compaction.into()),
        ]
        .into_iter()
        .flatten()
        .collect(),
        EventBody::ApprovalRequested { asking, request, .. } => {
            request.as_ref().map_or_else(|| vec![asking_needs(asking.as_deref())], |request| request.needs())
        }
        EventBody::AgentMode { .. } => vec![approvals::Capability::Modes.into()],
        EventBody::AgentCompacting { written_chars, .. } => {
            written_chars.map(|_| context::Capability::CompactionProgress.into()).into_iter().collect()
        }
        EventBody::AgentCompacted { asked, outcome, .. } => compacted_needs(asked.is_some(), outcome),
        // The hub records the first two itself, so they have no run to claim anything.
        EventBody::CompactionAsked { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::UserPrompt { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentUser { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::RequestFailed { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => Vec::new(),
    }
}

fn task_needs(kind: TaskKind) -> Capability {
    match kind {
        TaskKind::Subagent => background::Capability::Subagents.into(),
        TaskKind::Monitor | TaskKind::Command => background::Capability::BackgroundCommands.into(),
        TaskKind::Wait => background::Capability::ToolTasks.into(),
    }
}

fn asking_needs(asking: Option<&Asking>) -> Capability {
    match asking {
        Some(Asking::Questions { .. }) => asking::Capability::Questions.into(),
        Some(Asking::Plan { .. }) => asking::Capability::PlanApproval.into(),
        Some(Asking::ToolUse { .. } | Asking::Unknown) | None => approvals::Capability::Requests.into(),
    }
}

fn compacted_needs(asked: bool, outcome: &CompactionOutcome) -> Vec<Capability> {
    let mut needs = Vec::new();
    if asked {
        needs.push(context::Capability::Compaction.into());
    }
    if let CompactionOutcome::Compacted { by, before_tokens, summary, .. } = outcome {
        if *by == CompactedBy::Agent {
            needs.push(context::Capability::AutoCompaction.into());
        }
        if before_tokens.is_some() {
            needs.push(context::Capability::CompactionTokens.into());
        }
        if summary.is_some() {
            needs.push(context::Capability::CompactionSummary.into());
        }
    }
    needs
}
