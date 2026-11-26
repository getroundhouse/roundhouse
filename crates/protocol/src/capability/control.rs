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
use crate::worker::TOOLS_SERVER;

/// A request to a running agent, which the worker's `AgentControl` carries. What it names by a [`Seq`] is recorded
/// first. A launch is `AgentSpec`, with a [`conversation::Start`] and [`Settings`].
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
    /// Connect again to the tool server `server`, which the agent has and isn't connected to. Never Roundhouse's own, as
    /// [`Control::allowed`] says.
    Reconnect {
        server: ServerName,
    },
    /// Connect again to the tool server `server`, which the agent is connected to, and list its tools again. Never
    /// Roundhouse's own, as [`Control::allowed`] says.
    Reload {
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
    /// Report the models the chat can run as [`Report::Models`](super::Report::Models). An adapter whose agent pushes
    /// its models when a session starts or updates, rather than answering a query, reports the newest list the session
    /// gave it.
    ListModels,
    ListOutside,
    /// The person talks with the agent as `mode` says from now on, as the chat's conversation mode was turned on or
    /// off, which an adapter that says lines for the agent needs to know. Sent only when the mode changes, and not when
    /// the conversation moves to another device.
    Interaction {
        mode: crate::model::InteractionMode,
    },
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
            Control::Reconnect { .. } => one(setup::Capability::ToolServerReconnect.into()),
            Control::Reload { .. } => one(setup::Capability::ToolServerReload.into()),
            Control::SignInToServer { .. } => one(setup::Capability::ToolServerSignIn.into()),
            Control::SetGoal { .. } | Control::PauseGoal | Control::ClearGoal => one(turns::Capability::Goals.into()),
            Control::Review { .. } => one(turns::Capability::Reviews.into()),
            Control::ReadRules => one(approvals::Capability::Rules.into()),
            Control::ListModels => one(settings::Capability::DiscoveredModels.into()),
            Control::ListOutside => one(conversation::Capability::Import.into()),
            Control::Interaction { .. } => one(super::output::Capability::CoverLines.into()),
        }
    }

    /// Whether a run that claims `claims` can be asked this: every capability it needs, and what its parameters ask
    /// of the claims, such as a decision's parts, a change's settings, and something to keep only for a compaction
    /// that reads it. No run is asked to connect again to [`TOOLS_SERVER`], since Claude stops asking after that server's
    /// tasks for about 5 s, and until another reconnect works where one fails, which strands them. The hub refuses
    /// anything else before it's sent, so an adapter is never asked what it didn't claim.
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
        if let Control::Reconnect { server } | Control::Reload { server } = self {
            return server.as_str() != TOOLS_SERVER;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Capabilities, Claimed};
    use super::*;

    /// One it isn't connected to, and one it is, each by a claim of its own.
    #[test]
    fn a_run_is_asked_to_connect_again_only_where_it_claims_it_can_and_never_to_roundhouses_own_server() {
        let asks =
            |server: &str| [Control::Reconnect { server: server.into() }, Control::Reload { server: server.into() }];
        let allowed = |server: &str, claims: &Capabilities| asks(server).map(|control| control.allowed(claims));
        let mut claims = Capabilities::default();
        claims.setup.tool_servers = Some(Claimed {});
        assert_eq!(
            allowed("docs", &claims),
            [false, false],
            "a run of an earlier build only says how its servers stand"
        );

        claims.setup.tool_server_reconnect = Some(Claimed {});
        assert_eq!(allowed("docs", &claims), [true, false], "as Codex's, which leaves a connected one as it is");
        claims.setup.tool_server_reload = Some(Claimed {});

        assert_eq!(allowed("docs", &claims), [true, true]);
        assert_eq!(allowed(TOOLS_SERVER, &claims), [false, false]);
    }
}
