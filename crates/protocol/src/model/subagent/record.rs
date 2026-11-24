use serde::{Deserialize, Serialize};

use crate::attachment::Attachment;
use crate::event::{Denied, TaskOutcome};
use crate::model::{ModelName, Rev, SessionId, SubagentId, ToolUseId};

/// One subagent, named in its session by the ID its agent gives it, which its events name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Subagent {
    pub session_id: SessionId,
    /// Which subagent it is, by the ID its agent gives it, which its events name.
    pub id: SubagentId,
    /// The call that started it, which its row in the chat is drawn for.
    pub call: ToolUseId,
    /// Its kind of agent, such as `Explore`.
    #[serde(default)]
    pub kind: Option<String>,
    /// What it's for, in a few words.
    #[serde(default)]
    pub description: Option<String>,
    /// What it was asked.
    #[serde(default)]
    pub prompt: Option<String>,
    /// It works on while the agent carries on, rather than its call waiting for it.
    #[serde(default)]
    pub in_background: bool,
    /// The model its replies come from, as its newest reply named it. `None` until it's replied, and for one that
    /// replied before this was kept, whose stored row may not say.
    #[serde(default)]
    pub model: Option<ModelName>,
    pub status: SubagentStatus,
    /// When it started, or started again once it had ended, so how long it's been at it, or took, is from then.
    pub started_at_ms: i64,
    #[serde(default)]
    pub ended_at_ms: Option<i64>,
    /// About how many tokens its conversation holds, as its agent last counted them, as a context's `used_tokens` is,
    /// and not what its requests took in all. So it can fall, as when the subagent compacts. `None` until the agent
    /// says, from an agent that can't say what it holds, and for one that ended before this was kept, whose stored row
    /// may not say.
    #[serde(default)]
    pub context_tokens: Option<u64>,
    /// How many calls it's made.
    #[serde(default)]
    pub calls: u32,
    /// What it's done lately, which its call shows while it works, with each call typed, as
    /// [`crate::capability::calls`] says a call is stored. Only while it works: it's emptied when it ends. `None` until
    /// it's done something, and where the hub can't read what it stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lately: Option<Lately>,
    /// What it reported, once it finished.
    #[serde(default)]
    pub report: Option<String>,
    pub rev: Rev,
}

impl Subagent {
    /// The one of `subagents` that `named` names: the one with that ID, or else the one the call it names started,
    /// since a client names a subagent by its call until it has the subagent's record, as a tab opened on one before
    /// its agent said it started does. A read, a subscription or a search names a subagent so, and the hub finds it as
    /// this says, with SQL of its own, which its store's tests hold to this. A fake hub finds one by it. Only they use
    /// it, so it's behind `testing`.
    #[cfg(any(test, feature = "testing"))]
    pub fn named<'a>(subagents: &'a [Subagent], named: &SubagentId) -> Option<&'a Subagent> {
        let by_call = || subagents.iter().find(|subagent| SubagentId::of_call(&subagent.call) == *named);
        subagents.iter().find(|subagent| subagent.id == *named).or_else(by_call)
    }
}

/// Where a subagent has got to. One this build doesn't know reads as `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Working,
    Finished,
    Failed,
    /// The agent stopped it, or it went with the agent.
    Stopped,
    #[serde(other)]
    Unknown,
}

impl SubagentStatus {
    /// How it is once it ended with `outcome`.
    pub fn ended(outcome: TaskOutcome) -> Self {
        match outcome {
            TaskOutcome::Finished => SubagentStatus::Finished,
            TaskOutcome::Failed => SubagentStatus::Failed,
            TaskOutcome::Stopped => SubagentStatus::Stopped,
            TaskOutcome::Unknown => SubagentStatus::Unknown,
        }
    }
}

/// What a subagent has done lately: its newest calls, each typed, with the text it wrote just before it and what came
/// back once something has, then what it's written since the newest. Every change to a subagent's record goes to every
/// client, so the hub keeps only a few calls and cuts what each wrote and what came back short. A call is whole, so a
/// write's holds the file it wrote.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Lately {
    /// Oldest first.
    #[serde(default)]
    pub calls: Vec<LatelyCall>,
    /// What it's written since its newest call.
    #[serde(default)]
    pub since: Option<Said>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LatelyCall {
    /// What it wrote just before the call.
    #[serde(default)]
    pub before: Option<Said>,
    pub id: ToolUseId,
    pub call: crate::capability::calls::Call,
    /// What came back, once something has.
    #[serde(default)]
    pub result: Option<LatelyResult>,
}

/// What came back for a call, as its result block had it, but shorter where it was long. The hub may cut short each
/// text the output holds, such as a command's `output`, a read's `lines`, a page's `text`, an MCP tool's texts and a
/// search's lines and paths, keeping each one's start, and says so in `cut`. Everything else is as it came, such as a
/// read's `first` and `total`, a command's exit code and how it ended, and a search's count. What it changed in a file
/// is always left off, as `None`, since that can be as long as the file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LatelyResult {
    pub output: crate::capability::calls::CallOutput,
    #[serde(default)]
    pub is_error: bool,
    /// Some text of the output is only its start, which a client draws only when it says what went wrong, since a
    /// call's line counts what came back, and the start of a long read would say it read fewer lines than it did.
    #[serde(default)]
    pub cut: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<Attachment>,
    /// The person wouldn't let the call be made, as its result block said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied: Option<Denied>,
}

/// Text a subagent wrote, or its start, if it was long.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Said {
    pub text: String,
    /// `text` is only its start.
    #[serde(default)]
    pub cut: bool,
}
