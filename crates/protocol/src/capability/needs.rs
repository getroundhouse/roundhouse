//! The claims that need another, which a run that claims one has to claim too.

use super::{Capabilities, Capability};
use super::{approvals, asking, background, context, conversation, output, setup, turns};

/// Each claim that needs another, with those that meet its need, one of which a run that claims it claims too, as each
/// one's contract says: a fork is a conversation that can be resumed, an undo is of a rewind, a preview is of putting
/// files back, a fresh plan is an approved one, an overruling is of a reviewer's verdict, findings are a review's, a
/// compaction's progress, summary and tokens are of a compaction, asked for or the agent's own, the context's parts are
/// of its usage, a direct subagent is a subagent, and streamed reasoning is recorded reasoning. A claim in two rows needs
/// one of each, as signing in to a tool server needs its state and its request.
const NEEDS: &[(Capability, &[Capability])] = {
    use Capability::{Approvals, Asking, Background, Context, Conversation, Output, Setup, Turns};
    &[
        (Conversation(conversation::Capability::Fork), &[Conversation(conversation::Capability::Resume)]),
        (Conversation(conversation::Capability::UndoRewind), &[Conversation(conversation::Capability::Rewind)]),
        (
            Conversation(conversation::Capability::PreviewRestoreFiles),
            &[Conversation(conversation::Capability::RestoreFiles)],
        ),
        (Asking(asking::Capability::FreshPlan), &[Asking(asking::Capability::PlanApproval)]),
        (Approvals(approvals::Capability::Overrule), &[Approvals(approvals::Capability::ReviewVerdicts)]),
        (Turns(turns::Capability::ReviewFindings), &[Turns(turns::Capability::Reviews)]),
        (
            Context(context::Capability::CompactionProgress),
            &[Context(context::Capability::Compaction), Context(context::Capability::AutoCompaction)],
        ),
        (
            Context(context::Capability::CompactionSummary),
            &[Context(context::Capability::Compaction), Context(context::Capability::AutoCompaction)],
        ),
        (
            Context(context::Capability::CompactionTokens),
            &[Context(context::Capability::Compaction), Context(context::Capability::AutoCompaction)],
        ),
        (Context(context::Capability::UsageParts), &[Context(context::Capability::Usage)]),
        (Background(background::Capability::DirectSubagents), &[Background(background::Capability::Subagents)]),
        (Output(output::Capability::StreamThinking), &[Output(output::Capability::Thinking)]),
        (Setup(setup::Capability::ToolServerSignIn), &[Setup(setup::Capability::ToolServers)]),
        (Setup(setup::Capability::ToolServerSignIn), &[Asking(asking::Capability::ServerRequests)]),
    ]
};

impl Capabilities {
    /// What it claims whose need, as [`NEEDS`] lists them, it doesn't claim, each once. A run with none can be held
    /// to its claims.
    pub fn unmet_needs(&self) -> Vec<Capability> {
        let mut unmet = Vec::new();
        for (claim, met_by) in NEEDS {
            if self.has(*claim) && !met_by.iter().any(|need| self.has(*need)) && !unmet.contains(claim) {
                unmet.push(*claim);
            }
        }
        unmet
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::{Capabilities, Capability, conversation, setup};

    #[test]
    fn a_claim_that_needs_another_is_unmet_without_one_that_meets_it() {
        let stray = |claims| serde_json::from_value::<Capabilities>(claims).unwrap().unmet_needs();
        let fork = Capability::from(conversation::Capability::Fork);
        assert_eq!(stray(json!({"conversation": {"fork": {}}})), [fork]);
        assert_eq!(stray(json!({"conversation": {"fork": {}, "resume": {"ids": "given"}}})), []);
        let progress = json!({"context": {"compaction_progress": {}, "auto_compaction": {}}});
        assert_eq!(stray(progress), [], "a compaction the agent starts by itself is one");
        let sign_in = Capability::from(setup::Capability::ToolServerSignIn);
        assert_eq!(
            stray(json!({"setup": {"tool_server_sign_in": {}, "tool_servers": {}}})),
            [sign_in],
            "it needs both"
        );
        let both = json!({"setup": {"tool_server_sign_in": {}, "tool_servers": {}}, "asking": {"server_requests": {}}});
        assert_eq!(stray(both), []);
        assert_eq!(Capabilities::default().unmet_needs(), []);
    }
}
