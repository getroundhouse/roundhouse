//! What the agent says, and what it reports of its own work as it goes, apart from its calls.

use serde::{Deserialize, Serialize};

use super::Claimed;
use super::area::area;
use crate::model::{MessageId, ModelName, ToolUseId, ids};

area! { "the agent's output";
    /// The agent's words stream as it writes them, ahead of the event that records them, and that event holds exactly
    /// what streamed, so a client never shows words that change once they're recorded. Every agent's words are
    /// recorded, so this is only their streaming.
    StreamText => stream_text: Claimed,
    /// The model's reasoning is recorded beside its words, in the form a chat chooses of those [`Thinking`] offers.
    Thinking => thinking: Thinking,
    /// The model's reasoning streams as it's written, as its words do.
    StreamThinking => stream_thinking: Claimed,
    /// While the model's reasoning isn't shown, how much of it there is so far is, in tokens, so a long think doesn't
    /// look stuck. Each is shown as it comes, and replaced by the next.
    ThinkingProgress => thinking_progress: Claimed,
    /// The agent marks what it says along the way apart from its answer, so a client tells a turn's answer from its
    /// commentary by the mark rather than by where it falls, as Codex's phases say.
    Commentary => commentary: Claimed,
    /// The agent's todo list is reported whole each time it changes, each item with its words and status, and the hub
    /// keeps the newest on the session's record.
    Todos => todos: Claimed,
    /// The agent reports a turn's changes to files as they stand after each change, file by file, so a client shows
    /// what the turn changed without adding up its calls, and changes made by commands are counted too. The hub keeps
    /// the newest on the session's record.
    TurnChanges => turn_changes: Claimed,
    /// A reply that came from another model than the one chosen says so, with both models and why, such as a fallback
    /// from a model that refused, or one a mode runs on. The hub records it.
    Rerouting => rerouting: Claimed,
    /// A request the agent retries by itself is reported as it waits, with which attempt it is and how long until the
    /// next starts, so the turn doesn't look stuck. The hub records it.
    Retries => retries: Claimed,
    /// Words the agent takes back after showing them, as Claude does when a model's refusal falls back to another, are
    /// reported by their messages, which the hub records, and a client stops showing them.
    Retractions => retractions: Claimed,
    /// After a turn, the agent suggests what the person might send next, which a client can offer to send. The hub
    /// keeps the newest on the session's record until the next prompt.
    Suggestions => suggestions: Claimed,
    /// The agent sums up a run of its calls in a line, which the hub records, and a client can show in place of the
    /// calls.
    CallSummaries => call_summaries: Claimed,
    /// The agent can ask for the person's attention in words of its own, as Claude's `PushNotification` does, which the
    /// hub records, and a client shows as it does a chat that needs the person.
    Notifications => notifications: Claimed,
    /// While the agent can't answer a prompt said out loud, because it's busy or still thinking, the adapter says in a
    /// sentence, in the agent's voice, what it's doing, which a small model writes for it, as [`Covering`] says. Each is
    /// shown, and said by the device that has the conversation, as it comes, until the agent starts answering, and
    /// kept nowhere.
    CoverLines => cover_lines: Claimed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Thinking {
    /// The form a chat starts with.
    pub default: ThinkingForm,
    /// The others a chat can choose.
    #[serde(default)]
    pub others: Vec<ThinkingForm>,
}

impl Thinking {
    /// Whether `form` is one of them. One this build doesn't know is none, since it would match another unknown one.
    pub fn offers(&self, form: ThinkingForm) -> bool {
        form != ThinkingForm::Unknown && (self.default == form || self.others.contains(&form))
    }
}

/// How much of the model's reasoning is recorded.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingForm {
    /// All of it, as the model wrote it.
    Full,
    /// A summary the model's API writes of it, as Claude's `summarized` and Codex's reasoning summaries are.
    Summary,
    /// Its main points only, as Claude's `highlights` are.
    Highlights,
    /// None of it: the model still reasons, and only [`Capability::ThinkingProgress`] says how much.
    Hidden,
    #[serde(other)]
    Unknown,
}

/// The agent's todo list, whole, as it stands.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TodoList {
    pub items: Vec<Todo>,
    /// Why the list is as it is, in the agent's words, when it says, as Codex's plan updates can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    /// The agent's own ID for it, where its list has them, which [`Todo::after`] names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<TodoId>,
    pub text: String,
    pub status: TodoStatus,
    /// What it says while the item is in progress, such as "Running the tests", as Claude's `activeForm` does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// The items that have to be done first, by their IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<TodoId>,
}

ids!(
    /// One of the agent's todo items, by its own ID for it.
    TodoId,
);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
    /// Dropped without being done.
    Cancelled,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
    #[serde(other)]
    Unknown,
}

/// A reply from another model than the one chosen, as [`Capability::Rerouting`] reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Rerouted {
    /// The model chosen, as the agent named it.
    pub from: ModelName,
    pub to: ModelName,
    pub why: RerouteReason,
    /// Until when: the rest of this turn, or every turn after it too.
    pub lasts: RerouteLasts,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RerouteReason {
    /// The chosen model refused the request, and another answered it.
    Refused,
    /// The chosen model couldn't be reached, or was too busy, and the fallback answered.
    Unavailable,
    /// The request was judged risky, and a safer model answered, as Codex's cyber-safety rerouting does.
    Safety,
    /// The approval mode runs on a model of its own, as Claude's plan mode can.
    Mode,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RerouteLasts {
    Turn,
    Conversation,
    #[serde(other)]
    Unknown,
}

/// A request the agent is about to retry, as [`Capability::Retries`] reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Retrying {
    /// Which attempt is next, from 2.
    pub attempt: u32,
    /// How many the agent makes before it gives up, when it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub most: Option<u32>,
    /// How long until the next starts, from when the adapter reported it, when the agent says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_ms: Option<u64>,
    /// Why the last attempt failed.
    pub failure: crate::event::RequestFailure,
}

/// What the agent is doing, in a sentence in its own voice, said for it while it can't answer the prompt said out loud
/// that was recorded at `prompt`, as [`Capability::CoverLines`] reports it. It says only what the agent is doing, never
/// an answer, an opinion or anything the agent hasn't done. The adapter reports at most one for each prompt, and only
/// while the agent hasn't started answering it: nothing of the agent's own words since the prompt was sent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Covering {
    pub prompt: crate::model::Seq,
    pub line: String,
}

/// Messages the agent took back, as [`Capability::Retractions`] reports them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Retracted {
    pub messages: Vec<MessageId>,
}

/// A line summing up calls, as [`Capability::CallSummaries`] reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CallSummary {
    pub calls: Vec<ToolUseId>,
    pub summary: String,
}
