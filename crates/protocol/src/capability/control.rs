//! Everything the hub can ask of a running agent, each naming the capabilities it needs, so the hub refuses what a
//! run doesn't claim in one place, before anything reaches a worker.

use serde::{Deserialize, Serialize};

use super::approvals::{self, Decision};
use super::context::Keeping;
use super::conversation::PromptPoint;
use super::prompts::Extras;
use super::settings::{self, Settings};
use super::setup::ServerName;
use super::turns::{self, ReviewTarget};
use super::{Capabilities, Capability, background, calls, context, conversation, prompts, setup};
use crate::attachment::AttachmentFile;
use crate::model::{ApprovalId, Seq, TaskId};
use crate::prompt::Prompt;

/// A request to a running agent. What it names by a [`Seq`] is recorded first. It takes the place of the worker's
/// `AgentPrompt`, `AgentApproval`, `AgentCancel`, `AgentCompact` and `AgentConfigure`, as one request that carries a
/// `Control`, and a launch is `AgentSpec`, with a [`conversation::Start`] and [`Settings`].
///
/// It has no variant for one this build doesn't know, since each new one, and every new value of one, needs a
/// capability of its own, which no run from an older build claims, so the hub never sends one to a worker that can't
/// read it. The account, its limits and the context aren't asked for here: the account is read by a one-off request
/// that starts no conversation, and the limits and the context come as the agent works, as reports.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "control", rename_all = "snake_case")]
pub enum Control {
    /// The prompt recorded at `seq`, each of whose files has been written into the worktree first. Every agent takes
    /// one while it's idle, and the hub sends one while a turn runs only to a run that claims
    /// [`prompts::Capability::Steer`].
    Prompt {
        seq: Seq,
        prompt: Prompt<AttachmentFile>,
        #[serde(default)]
        extras: Extras,
    },
    /// The prompt recorded at `seq`, added to the conversation without starting a turn.
    Note {
        seq: Seq,
        prompt: Prompt<AttachmentFile>,
    },
    /// The question recorded at `asked`, answered from the conversation as it stands, without adding to it.
    Aside {
        asked: Seq,
        question: String,
    },
    Cancel,
    Decide {
        approval: ApprovalId,
        decision: Decision,
    },
    /// Let a use a reviewer refused go ahead.
    Overrule {
        approval: ApprovalId,
    },
    Compact {
        asked: Seq,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keeping: Option<String>,
    },
    Change(Settings),
    /// Take the conversation back to before the prompt `before`.
    Rewind {
        before: PromptPoint,
    },
    UndoRewind,
    /// Put back the files the agent changed since the prompt `since`.
    RestoreFiles {
        since: PromptPoint,
    },
    /// Say what putting back the files the agent changed since the prompt `since` would change, and change nothing.
    PreviewRestoreFiles {
        since: PromptPoint,
    },
    /// Run the command recorded at `asked` for the person, in the conversation.
    RunCommand {
        asked: Seq,
        command: String,
    },
    /// Stop a background task, a schedule included.
    StopTask {
        task: TaskId,
    },
    ReadTaskOutput {
        task: TaskId,
    },
    Reconnect {
        server: ServerName,
    },
    SignInToServer {
        server: ServerName,
    },
    SetGoal {
        objective: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budget_tokens: Option<u64>,
    },
    PauseGoal,
    ClearGoal,
    /// Start a turn that reviews `target`, as the request recorded at `asked` asks.
    Review {
        asked: Seq,
        target: ReviewTarget,
    },
    ReadRules,
    ListModels,
    ListOutside,
}

impl Control {
    /// The capabilities it needs, which are none for a plain prompt, since every agent takes one.
    pub fn needs(&self) -> Vec<Capability> {
        let one = |capability: Capability| vec![capability];
        match self {
            Control::Prompt { extras, .. } => extras.needs(),
            Control::Note { .. } => one(prompts::Capability::Notes.into()),
            Control::Aside { .. } => one(prompts::Capability::Asides.into()),
            Control::Cancel => one(turns::Capability::Cancel.into()),
            Control::Decide { decision, .. } => decision.needs(),
            Control::Overrule { .. } => one(approvals::Capability::Overrule.into()),
            Control::Compact { .. } => one(context::Capability::Compaction.into()),
            Control::Change(settings) => {
                let mut needs = settings.needs();
                needs.push(settings::Capability::LiveChanges.into());
                needs
            }
            Control::Rewind { .. } => one(conversation::Capability::Rewind.into()),
            Control::UndoRewind => one(conversation::Capability::UndoRewind.into()),
            Control::RestoreFiles { .. } => one(conversation::Capability::RestoreFiles.into()),
            Control::PreviewRestoreFiles { .. } => one(conversation::Capability::PreviewRestoreFiles.into()),
            Control::RunCommand { .. } => one(calls::Capability::PersonCommands.into()),
            Control::StopTask { .. } => one(background::Capability::StopTasks.into()),
            Control::ReadTaskOutput { .. } => one(background::Capability::TaskOutput.into()),
            Control::Reconnect { .. } => one(setup::Capability::ToolServers.into()),
            Control::SignInToServer { .. } => one(setup::Capability::ToolServerSignIn.into()),
            Control::SetGoal { .. } | Control::PauseGoal | Control::ClearGoal => one(turns::Capability::Goals.into()),
            Control::Review { .. } => one(turns::Capability::Reviews.into()),
            Control::ReadRules => one(approvals::Capability::Rules.into()),
            Control::ListModels => one(settings::Capability::DiscoveredModels.into()),
            Control::ListOutside => one(conversation::Capability::Import.into()),
        }
    }

    /// Whether a run that claims `claims` can be asked this: every capability it needs, and what its parameters ask
    /// of the claims, such as a decision's parts, a change's settings, and something to keep only for a compaction
    /// that reads it. The hub refuses anything else before it's sent, so an adapter is never asked what it didn't
    /// claim.
    pub fn allowed(&self, claims: &Capabilities) -> bool {
        if !claims.has_all(self.needs()) {
            return false;
        }
        if let Control::Decide { decision, .. } = self {
            return decision.allowed(claims);
        }
        if let Control::Change(settings) = self {
            return settings.allowed_live(claims);
        }
        if let Control::Compact { keeping: Some(_), .. } = self {
            return claims.context.compaction.is_some_and(|compaction| compaction.keeping == Keeping::Read);
        }
        true
    }
}
