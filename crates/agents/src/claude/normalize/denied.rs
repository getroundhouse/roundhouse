//! Which of a `user` line's results answer a tool's use the person denied, as the CLI says beside them in
//! `tool_result_meta`, and what the person said. `docs/claude-cli.md` has what the CLI writes there.

use protocol::event::{CallKind, ContentBlock, Denied, EventBody};
use serde_json::Value;

use crate::claude::approvals::denied_in_its_own_words;

/// Has each result in `body` to a tool's use the person denied say so, from `line`, the line it was read from. A result
/// to questions or a plan isn't one, since what became of those is their approval's to say. The kinds of the calls the
/// results answer have to be known already, as [`super::kinds::say`] says them.
pub(super) fn say(body: &mut EventBody, line: &Value) {
    let EventBody::AgentUser { blocks, .. } = body else { return };
    let Some(meta) = line.get("tool_result_meta").and_then(Value::as_array) else { return };
    for block in blocks {
        let ContentBlock::ToolResult { tool_use_id, text, call, denied, .. } = block else { continue };
        if matches!(call, Some(CallKind::Questions | CallKind::Plan)) {
            continue;
        }
        let about = |entry: &&Value| entry.get("id").and_then(Value::as_str) == Some(tool_use_id.as_str());
        if let Some(rejected) = meta.iter().find(about).and_then(|entry| rejected(entry, text)) {
            *denied = Some(rejected);
        }
    }
}

/// The denial `entry` says of the call whose result says `text`, if the person denied it, or `None` where they didn't.
/// Only a decision to reject from the person is one: a request a stop cancelled says `user-rejected` as its
/// `non_execution_kind` and no decision, and what became of it is its approval's to say.
fn rejected(entry: &Value, text: &str) -> Option<Denied> {
    let decision = entry.get("permission_decision")?;
    let by_the_person = decision.get("decision").and_then(Value::as_str) == Some("reject")
        && decision.get("source").and_then(Value::as_str) == Some("user_reject");
    // What the agent was told is what the adapter sent, which is the person's words unless it's the adapter's own.
    let theirs = !text.trim().is_empty() && !denied_in_its_own_words(text);
    by_the_person.then(|| Denied { said: theirs.then(|| text.to_string()) })
}

#[cfg(test)]
mod tests {
    use protocol::event::{CallKind, ContentBlock, Denied, EventBody};
    use serde_json::{Value, json};

    use super::say;

    fn denied_by(meta: &Value, text: &str, call: Option<CallKind>) -> Option<Denied> {
        let result = ContentBlock::ToolResult {
            tool_use_id: "c".into(),
            is_error: true,
            text: text.into(),
            images: Vec::new(),
            change: None,
            encrypted: false,
            call,
            exit_code: None,
            denied: None,
        };
        let mut body = EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![result] };
        say(&mut body, &json!({ "type": "user", "tool_result_meta": [meta] }));
        let EventBody::AgentUser { blocks, .. } = body else { return None };
        let [ContentBlock::ToolResult { denied, .. }] = blocks.as_slice() else { return None };
        denied.clone()
    }

    fn decided(decision: &str, source: &str) -> Value {
        json!({ "id": "c", "permission_decision": { "decision": decision, "source": source } })
    }

    #[test]
    fn a_call_the_person_rejected_is_denied_with_their_words_unless_they_said_none() {
        let command = Some(CallKind::Command);
        let rejected = decided("reject", "user_reject");

        let said = Denied { said: Some("Not that".into()) };
        assert_eq!(denied_by(&rejected, "Not that", command.clone()), Some(said));
        assert_eq!(denied_by(&rejected, "The user denied this.", command.clone()), Some(Denied::default()));
        let as_captured = json!({
            "id": "c",
            "non_execution_kind": "permission-rule",
            "permission_decision": { "decision": "reject", "source": "user_reject" },
        });
        assert_eq!(
            denied_by(&as_captured, "Not that", command.clone()),
            Some(Denied { said: Some("Not that".into()) })
        );
        assert_eq!(denied_by(&rejected, "Not that", None), Some(Denied { said: Some("Not that".into()) }));
    }

    #[test]
    fn what_was_allowed_cancelled_decided_by_anyone_else_another_call_or_questions_and_a_plan_arent() {
        let command = Some(CallKind::Command);
        let rejected = decided("reject", "user_reject");

        assert_eq!(denied_by(&decided("accept", "user_temporary"), "ok", command.clone()), None);
        let cancelled = json!({ "id": "c", "non_execution_kind": "user-rejected" });
        let the_clis = "The user doesn't want to proceed with this tool use.";
        assert_eq!(denied_by(&cancelled, the_clis, command.clone()), None, "a stop cancelled it");
        assert_eq!(denied_by(&decided("reject", "config"), "No", command.clone()), None);
        let another =
            json!({ "id": "other", "permission_decision": { "decision": "reject", "source": "user_reject" } });
        assert_eq!(denied_by(&another, "Not that", command), None);
        assert_eq!(denied_by(&rejected, "Skip these", Some(CallKind::Questions)), None);
        assert_eq!(denied_by(&rejected, "Keep planning", Some(CallKind::Plan)), None);
    }
}
