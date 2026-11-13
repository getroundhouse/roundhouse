use super::super::{id_of, str_of};
use protocol::event::{EventBody, FailureKind, NoticeLevel, RequestFailure};
use protocol::model::ToolUseId;
use serde_json::Value;

/// The CLI's name for the model of a message it wrote itself, such as one saying the API failed. No model wrote it.
const NO_MODEL: &str = "<synthetic>";

/// An `assistant` line the CLI wrote itself, if this is one: a failed request when it says it wraps an error from the
/// model's API, and a note otherwise. Either keeps the subagent it's from.
pub(in super::super) fn message(value: &Value) -> Option<EventBody> {
    let message = value.get("message")?;
    if str_of(message, "model") != Some(NO_MODEL) {
        return None;
    }
    let text = text_of(message.get("content"));
    let parent_tool_use_id: Option<ToolUseId> = id_of(value, "parent_tool_use_id");
    if value.get("is_api_error_message").and_then(Value::as_bool) != Some(true) {
        return Some(EventBody::Notice { level: NoticeLevel::Info, message: text, parent_tool_use_id });
    }
    let limit = value.get("api_error_params").and_then(|params| params.get("rate_limit_info"));
    let resets_at_ms =
        limit.and_then(|limit| limit.get("resetsAt")).and_then(Value::as_i64).map(|s| s.saturating_mul(1000));
    let kind = str_of(value, "error").map_or(FailureKind::Unknown, kind_of);
    Some(EventBody::RequestFailed { parent_tool_use_id, failure: RequestFailure { message: text, kind, resets_at_ms } })
}

/// The kind of failure the CLI's `error` says, from the categories its schema lists, rather than its words, which can
/// change. Its finer `api_error` says more than a title needs. A category it adds is `Unknown`.
fn kind_of(error: &str) -> FailureKind {
    match error {
        "rate_limit" => FailureKind::Limit,
        "authentication_failed" | "oauth_org_not_allowed" | "verification_required" | "cloud_credential_error" => {
            FailureKind::SignIn
        }
        "account_on_hold" | "billing_error" => FailureKind::Account,
        "overloaded" => FailureKind::Overloaded,
        "server_error" => FailureKind::ServerError,
        "invalid_request" | "model_not_found" => FailureKind::Refused,
        _ => FailureKind::Unknown,
    }
}

/// Which of a command's streams it printed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Stream {
    Out,
    Err,
}

/// What a command the CLI ran printed, as it was printed, if this is a replayed `user` line of it, such as what
/// choosing another model said. It's in tags that say which stream it was on.
pub(in super::super) fn printed(value: &Value) -> Option<(Stream, String)> {
    if value.get("isReplay").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let text = text_of(value.get("message").and_then(|message| message.get("content")));
    let text = text.trim();
    let within = |tag: &str| text.strip_prefix(&format!("<{tag}>"))?.strip_suffix(&format!("</{tag}>")).map(str::trim);
    let (stream, said) = match within("local-command-stdout") {
        Some(said) => (Stream::Out, said),
        None => (Stream::Err, within("local-command-stderr")?),
    };
    Some((stream, said.to_string()))
}

/// What a replayed line that `printed` reads says, as a notice: a warning for what went to stderr.
pub(in super::super) fn printed_notice(stream: Stream, text: String) -> EventBody {
    let level = match stream {
        Stream::Out => NoticeLevel::Info,
        Stream::Err => NoticeLevel::Warning,
    };
    EventBody::Notice { level, message: text, parent_tool_use_id: None }
}

/// The text of a message's content, its text blocks run together.
pub(in super::super) fn text_of(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks.iter().filter_map(|block| str_of(block, "text")).collect(),
        _ => String::new(),
    }
}

/// Whether the newest the agent has said in the turn going is its own failed request, so a turn that fails now ended
/// on it, and has nothing to add to what the failure says.
#[derive(Debug, Default)]
pub(in super::super) struct FailedRequests {
    pub(super) last: bool,
}

impl FailedRequests {
    /// Notes a failed request of the agent's own, and anything the agent says after it, which goes on past it. A
    /// subagent's failure, and what a subagent says, leave the agent where it was.
    pub(in super::super) fn note(&mut self, body: &EventBody) {
        if matches!(body, EventBody::RequestFailed { parent_tool_use_id: None, .. }) {
            self.last = true;
        } else if matches!(
            body,
            EventBody::Assistant { parent_tool_use_id: None, .. }
                | EventBody::AgentUser { parent_tool_use_id: None, .. }
        ) {
            self.last = false;
        }
    }

    /// Whether the turn that's ending came to an end on the agent's own failed request, with nothing of the agent's
    /// after it. The CLI ends a turn on one for several reasons, such as an error from the API, a conversation too full
    /// to compact, or an image it can't send, so the reason it gives doesn't say. The next turn starts with none.
    pub(in super::super) fn ended_on_one(&mut self) -> bool {
        std::mem::take(&mut self.last)
    }
}
