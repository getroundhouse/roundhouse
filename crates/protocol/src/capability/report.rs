//! What an agent says that [`EventBody`](crate::event::EventBody) has no kind for, each naming the capability it needs
//! and saying who keeps it, so the hub can drop one its run doesn't claim, and record one the log keeps as an event.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::account::{Credits, Limit};
use super::approvals::{Denied, Rule, Verdict};
use super::calls::output::FileOutcome;
use super::conversation::Outside;
use super::output::{CallSummary, Rerouted, Retracted, Retrying, TodoList};
use super::prompts::{AgentCommand, ListedSkill};
use super::settings::{ModelOffer, Persona, Speed, StyleName};
use super::setup::{HookRun, ToolServerState};
use super::turns::{Finding, Goal, OwnTurn};
use super::{Capability, account, approvals, background, calls, context, conversation, output, prompts, settings};
use super::{setup, turns};
use crate::model::{ApprovalId, CacheRequest, ContextUsage, Seq, TaskId, ToolUseId};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "report", rename_all = "snake_case")]
pub enum Report {
    /// The agent's commands, whole.
    Commands {
        commands: Vec<AgentCommand>,
    },
    Skills {
        skills: Vec<ListedSkill>,
    },
    Personas {
        personas: Vec<Persona>,
    },
    Styles {
        styles: Vec<StyleName>,
    },
    /// The models the chat can run, whole.
    Models {
        models: Vec<ModelOffer>,
    },
    /// The speed the agent runs at, when it isn't the one chosen, or `None` once it is again.
    Speed {
        speed: Option<Speed>,
    },
    /// The answer to the aside recorded at `asked`.
    Aside {
        asked: Seq,
        answer: String,
    },
    ThinkingProgress {
        tokens: u64,
    },
    Todos(TodoList),
    /// What the turn has changed so far, file by file, whole.
    TurnChanges {
        files: Vec<FileOutcome>,
    },
    Rerouted(Rerouted),
    Retrying(Retrying),
    Retracted(Retracted),
    Suggestion {
        prompt: String,
    },
    CallSummary(CallSummary),
    Notification {
        text: String,
    },
    /// What the running call `call` has done since it last said.
    CallProgress {
        call: ToolUseId,
        progress: Progress,
    },
    /// A reviewer's verdict on the request `approval`, which resolves it.
    Verdict {
        approval: ApprovalId,
        verdict: Verdict,
    },
    Denied(Denied),
    /// The rules the agent goes by, whole.
    Rules {
        rules: Vec<Rule>,
    },
    /// The conversation was taken back to before the prompt recorded at `before`, and these files were put back, for a
    /// run whose rewinds put them back.
    Rewound {
        before: Seq,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<FileOutcome>,
    },
    RewindUndone,
    /// The files that were put back.
    FilesRestored {
        files: Vec<FileOutcome>,
    },
    /// The files that would be put back, and how many lines they'd gain and lose in all, where the agent says.
    WouldRestoreFiles {
        paths: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        insertions: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deletions: Option<u32>,
    },
    /// Why the agent started the turn that's starting.
    OwnTurn {
        why: OwnTurn,
    },
    /// The chat's goal as it stands, or `None` once there's none.
    Goal {
        goal: Option<Goal>,
    },
    Findings {
        findings: Vec<Finding>,
    },
    /// The turn's last reply, as the JSON its prompt's schema asked for.
    Reply {
        json: Value,
    },
    /// What a background task has printed so far.
    TaskOutput {
        task: TaskId,
        output: String,
    },
    /// Calls whose results were dropped from the context.
    Pruned {
        calls: Vec<ToolUseId>,
    },
    InstructionFiles {
        paths: Vec<String>,
    },
    ToolServers {
        servers: Vec<ToolServerState>,
    },
    Hook(HookRun),
    /// What the adapter's own asking found of the account's limits. What the agent hears of them in its conversation
    /// is an event, which the log keeps.
    Limits {
        limits: Vec<Limit>,
    },
    Credits(Credits),
    /// How full the context is, as of the agent's newest request to its model.
    Context {
        context: ContextUsage,
    },
    /// What one of the agent's own requests to its model read from the API's cache and wrote to it, as the model
    /// started to answer it.
    Cache {
        request: CacheRequest,
    },
    /// The conversations the agent holds that Roundhouse didn't start, whole.
    Outside {
        conversations: Vec<Outside>,
    },
    #[serde(other)]
    Unknown,
}

/// What a running call has done since it last said.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "progress", rename_all = "snake_case")]
pub enum Progress {
    /// What a command printed since.
    Output { text: String },
    /// A line saying how far it has got, such as a tool server's.
    Message { text: String },
    /// The input it's being given, as much as has streamed.
    Input { json: String },
    #[serde(other)]
    Unknown,
}

/// Who keeps a report, which is what makes it the hub's state rather than the agent's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keeper {
    /// It's an event in the session's log, so every client sees it, and so does a client that comes later.
    Log,
    /// Only the newest matters, which the hub keeps on the session's record, and announces when it changes.
    Session,
    /// Only the newest matters, which the hub keeps on the account's record, since agents in any workspace can run
    /// under the account.
    Account,
    /// It's shown as it comes, and kept nowhere, as a streamed reply is, since what it shows is recorded once it's
    /// done.
    Shown,
    /// It answers the hub's own asking, and nothing keeps it.
    Answer,
}

impl Report {
    /// The capability it needs, which its adapter claims, or `None` for one this build doesn't know, which the hub
    /// drops.
    pub fn needs(&self) -> Option<Capability> {
        let needs: Capability = match self {
            Report::Commands { .. } => prompts::Capability::Commands.into(),
            Report::Skills { .. } => prompts::Capability::Skills.into(),
            Report::Personas { .. } => settings::Capability::Personas.into(),
            Report::Styles { .. } => settings::Capability::Styles.into(),
            Report::Models { .. } => settings::Capability::DiscoveredModels.into(),
            Report::Speed { .. } => settings::Capability::Speed.into(),
            Report::Aside { .. } => prompts::Capability::Asides.into(),
            Report::ThinkingProgress { .. } => output::Capability::ThinkingProgress.into(),
            Report::Todos(_) => output::Capability::Todos.into(),
            Report::TurnChanges { .. } => output::Capability::TurnChanges.into(),
            Report::Rerouted(_) => output::Capability::Rerouting.into(),
            Report::Retrying(_) => output::Capability::Retries.into(),
            Report::Retracted(_) => output::Capability::Retractions.into(),
            Report::Suggestion { .. } => output::Capability::Suggestions.into(),
            Report::CallSummary(_) => output::Capability::CallSummaries.into(),
            Report::Notification { .. } => output::Capability::Notifications.into(),
            Report::CallProgress { progress: Progress::Input { .. }, .. } => calls::Capability::StreamInput.into(),
            Report::CallProgress { .. } => calls::Capability::Progress.into(),
            Report::Verdict { .. } => approvals::Capability::ReviewVerdicts.into(),
            Report::Denied(_) => approvals::Capability::Denials.into(),
            Report::Rules { .. } => approvals::Capability::Rules.into(),
            Report::Rewound { .. } => conversation::Capability::Rewind.into(),
            Report::RewindUndone => conversation::Capability::UndoRewind.into(),
            Report::FilesRestored { .. } => conversation::Capability::RestoreFiles.into(),
            Report::WouldRestoreFiles { .. } => conversation::Capability::PreviewRestoreFiles.into(),
            Report::OwnTurn { .. } => turns::Capability::OwnTurns.into(),
            Report::Goal { .. } => turns::Capability::Goals.into(),
            Report::Findings { .. } => turns::Capability::ReviewFindings.into(),
            Report::Reply { .. } => prompts::Capability::StructuredReply.into(),
            Report::TaskOutput { .. } => background::Capability::TaskOutput.into(),
            Report::Pruned { .. } => context::Capability::Pruning.into(),
            Report::InstructionFiles { .. } => setup::Capability::InstructionFiles.into(),
            Report::ToolServers { .. } => setup::Capability::ToolServers.into(),
            Report::Hook(_) => setup::Capability::Hooks.into(),
            Report::Limits { .. } => account::Capability::Limits.into(),
            Report::Credits(_) => account::Capability::Credits.into(),
            Report::Context { .. } => context::Capability::Usage.into(),
            Report::Cache { .. } => context::Capability::Cache.into(),
            Report::Outside { .. } => conversation::Capability::Import.into(),
            Report::Unknown => return None,
        };
        Some(needs)
    }

    /// Who keeps it.
    pub fn keeper(&self) -> Keeper {
        match self {
            Report::Aside { .. }
            | Report::Rerouted(_)
            | Report::Retrying(_)
            | Report::Retracted(_)
            | Report::CallSummary(_)
            | Report::Notification { .. }
            | Report::Verdict { .. }
            | Report::Denied(_)
            | Report::Rewound { .. }
            | Report::RewindUndone
            | Report::FilesRestored { .. }
            | Report::OwnTurn { .. }
            | Report::Findings { .. }
            | Report::Reply { .. }
            | Report::Pruned { .. }
            | Report::Hook(_) => Keeper::Log,
            Report::Commands { .. }
            | Report::Skills { .. }
            | Report::Personas { .. }
            | Report::Styles { .. }
            | Report::Speed { .. }
            | Report::Todos(_)
            | Report::TurnChanges { .. }
            | Report::Suggestion { .. }
            | Report::Rules { .. }
            | Report::Models { .. }
            | Report::Goal { .. }
            | Report::InstructionFiles { .. }
            | Report::ToolServers { .. }
            | Report::Context { .. }
            | Report::Cache { .. } => Keeper::Session,
            Report::Limits { .. } | Report::Credits(_) => Keeper::Account,
            Report::ThinkingProgress { .. } | Report::CallProgress { .. } => Keeper::Shown,
            Report::WouldRestoreFiles { .. } | Report::TaskOutput { .. } | Report::Outside { .. } | Report::Unknown => {
                Keeper::Answer
            }
        }
    }
}
