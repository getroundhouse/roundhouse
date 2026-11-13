//! A subagent's record: what it was asked, where it's got to, and what it reported, which the hub keeps as it records
//! the events that say, so a client can show a subagent without reading its work.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ModelName, Rev, SessionId, ToolUseId};
use crate::attachment::Attachment;
use crate::event::{CallKind, ContentBlock, Denied, SubagentOutcome};

/// One subagent, named by its session and the call that started it, which its events name as their parent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Subagent {
    pub session_id: SessionId,
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
    /// The model its replies come from, as its newest reply named it. `None` until it's replied, and from a hub from
    /// before this was kept.
    #[serde(default)]
    pub model: Option<ModelName>,
    pub status: SubagentStatus,
    pub started_at_ms: i64,
    #[serde(default)]
    pub ended_at_ms: Option<i64>,
    /// About how many tokens its conversation holds, as its agent last counted them, as a context's `used_tokens` is,
    /// and not what its requests took in all. So it can fall, as when the subagent compacts. `None` until the agent
    /// says, from an agent that can't say what it holds, and from a hub from before this was kept.
    #[serde(default)]
    pub context_tokens: Option<u64>,
    /// How many calls it's made.
    #[serde(default)]
    pub calls: u32,
    /// The newest of them. A client from before `recent` shows this, so it's still kept.
    #[serde(default)]
    pub latest_call: Option<Call>,
    /// What it's done lately, which its call shows while it works. Only while it works: it's emptied when it ends.
    #[serde(default)]
    pub recent: RecentWork,
    /// What it reported, once it finished.
    #[serde(default)]
    pub report: Option<String>,
    #[serde(default)]
    pub rev: Rev,
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
    pub fn ended(outcome: SubagentOutcome) -> Self {
        match outcome {
            SubagentOutcome::Finished => SubagentStatus::Finished,
            SubagentOutcome::Failed => SubagentStatus::Failed,
            SubagentOutcome::Stopped => SubagentStatus::Stopped,
            SubagentOutcome::Unknown => SubagentStatus::Unknown,
        }
    }
}

/// A call an agent or subagent made: which tool, with what.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Call {
    pub id: ToolUseId,
    pub name: String,
    pub input: Value,
    /// What kind of call it is, as its `ToolUse` said. `None` for one recorded before adapters said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<CallKind>,
}

impl Call {
    /// The call `block` makes, as a record keeps it, or `None` for any other block.
    pub fn of(block: &ContentBlock) -> Option<Self> {
        let ContentBlock::ToolUse { id, name, input, call } = block else { return None };
        Some(Self { id: id.clone(), name: name.clone(), input: input.clone(), kind: call.clone() })
    }
}

/// What a subagent has done lately: its newest calls, each with the text it wrote just before it and what came back
/// once something has, then what it's written since the newest. Every change to a subagent's record goes to every
/// client, so the hub keeps only a few calls and cuts each text short. A call's input is whole, as `latest_call`'s is,
/// so a write's holds the file it wrote.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RecentWork {
    /// Oldest first.
    #[serde(default)]
    pub calls: Vec<RecentCall>,
    /// What it's written since its newest call.
    #[serde(default)]
    pub since: Option<Said>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RecentCall {
    /// What it wrote just before the call.
    #[serde(default)]
    pub before: Option<Said>,
    pub call: Call,
    /// What came back, once something has.
    #[serde(default)]
    pub result: Option<CallResult>,
}

/// Text a subagent wrote, or its start, if it was long.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Said {
    pub text: String,
    /// `text` is only its start.
    #[serde(default)]
    pub cut: bool,
}

/// What came back for a call, as its result block had it, or the start of it, if it was long. What it changed in a
/// file isn't kept, since that can be as long as the file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CallResult {
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub text: String,
    /// `text` is only its start, which a client draws only when it says what went wrong, since a call's line counts
    /// what came back, and the start of a long read would say it read fewer lines than it did.
    #[serde(default)]
    pub cut: bool,
    #[serde(default)]
    pub images: Vec<Attachment>,
    /// What came back is encrypted, for the model alone, so `text` holds none of it.
    #[serde(default)]
    pub encrypted: bool,
    /// How the command exited, as its result block said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// The person wouldn't let the call be made, as its result block said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied: Option<Denied>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::fixtures;

    /// A record as a hub from before `recent` sent it.
    fn older() -> Value {
        json!({ "session_id": "s", "call": "c", "status": "working", "started_at_ms": 1, "calls": 1,
            "latest_call": { "id": "read", "name": "Read", "input": { "file_path": "a.rs" } } })
    }

    #[test]
    fn a_record_from_an_older_hub_has_done_nothing_lately_that_it_says() {
        let subagent: Subagent = serde_json::from_value(older()).unwrap();

        assert_eq!(subagent.recent, RecentWork::default());
        assert_eq!(subagent.context_tokens, None);
        assert_eq!(subagent.latest_call.map(|call| call.name), Some("Read".to_string()));
    }

    /// Its form on the wire and in the hub's column, which a change of shape would fail.
    #[test]
    fn what_a_subagent_has_done_lately_has_one_form() {
        let call = fixtures::call("read", "Read", json!({ "file_path": "a.rs" }));
        let result = CallResult { cut: true, ..fixtures::call_result("1→fn main") };
        let before = Some(Said { text: "I'll read it.".into(), cut: false });
        let since = Some(Said { text: "It's there.".into(), cut: false });
        let work = RecentWork { calls: vec![RecentCall { before, call, result: Some(result) }], since };

        let form = json!({
            "calls": [{
                "before": { "text": "I'll read it.", "cut": false },
                "call": { "id": "read", "name": "Read", "input": { "file_path": "a.rs" } },
                "result": { "is_error": false, "text": "1→fn main", "cut": true, "images": [],
                    "encrypted": false },
            }],
            "since": { "text": "It's there.", "cut": false },
        });
        assert_eq!(serde_json::to_value(&work).unwrap(), form);
        assert_eq!(serde_json::from_value::<RecentWork>(form).unwrap(), work);
        let sparse = json!({ "calls": [{ "call": { "id": "read", "name": "Read", "input": {} } }] });
        assert_eq!(serde_json::from_value::<RecentWork>(sparse).unwrap().calls[0].result, None, "only what it has");
    }
}
