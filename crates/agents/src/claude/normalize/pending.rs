//! Calls to tools the API runs itself, such as the advisor, that have had no result, and which of them the agent went
//! on without.
//!
//! The API's reference calls such a call pending when its message ends for another reason, such as to hand the CLI a
//! call to one of the agent's own tools. The CLI has been seen to send the message back without the call, so to the
//! model it was never made, and nothing in the stream says so. What shows it is the sender's next message beginning
//! with something other than the call's result, which is where the result of a call still pending would be expected.
//! `docs/claude-cli.md` has what was seen, and what hasn't been.

use std::collections::HashMap;

use protocol::model::{MessageId, ToolUseId};
use serde_json::Value;

use super::{id_of, str_of};

/// The calls waiting on a result, by who made them: the agent, or the subagent started by a call. Their messages
/// arrive mixed together.
#[derive(Debug, Default)]
pub(super) struct PendingCalls(HashMap<Option<ToolUseId>, Pending>);

/// One sender's calls with no result, all from its newest message that made any.
#[derive(Debug)]
struct Pending {
    message: MessageId,
    calls: Vec<ToolUseId>,
}

impl PendingCalls {
    /// Notes the calls an `assistant` line makes and the results it holds, and returns the calls it shows were
    /// dropped: those its sender made in an earlier message that are still waiting, when this line holds something
    /// other than a result for one of them. Any other line shows nothing.
    pub(super) fn note(&mut self, line: &Value) -> Vec<ToolUseId> {
        let message = line.get("message").unwrap_or(&Value::Null);
        let (Some("assistant"), Some(id)) = (str_of(line, "type"), id_of::<MessageId>(message, "id")) else {
            return Vec::new();
        };
        let sender: Option<ToolUseId> = id_of(line, "parent_tool_use_id");
        let blocks = message.get("content").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);

        let mut dropped = Vec::new();
        if let Some(pending) = self.0.get_mut(&sender) {
            // A result names its call whatever kind of result it is, so one this build can't read still counts.
            let waiting = pending.calls.len();
            pending
                .calls
                .retain(|call| !blocks.iter().any(|block| str_of(block, "tool_use_id") == Some(call.as_str())));
            let answered = waiting - pending.calls.len();
            if pending.message != id && answered < blocks.len() {
                dropped = std::mem::take(&mut pending.calls);
            }
            if pending.calls.is_empty() {
                self.0.remove(&sender);
            }
        }
        let made = blocks.iter().filter(|block| str_of(block, "type") == Some("server_tool_use"));
        let made: Vec<ToolUseId> = made.filter_map(|block| id_of(block, "id")).collect();
        if !made.is_empty() {
            let pending = self.0.entry(sender).or_insert_with(|| Pending { message: id.clone(), calls: Vec::new() });
            pending.message = id;
            pending.calls.extend(made);
        }
        dropped
    }

    /// The agent's turn ended, which says as much about the calls it left waiting as anything could. A subagent
    /// works on across the agent's turns, so its calls stay.
    pub(super) fn turn_ended(&mut self) {
        self.0.remove(&None);
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::EventBody;
    use serde_json::{Value, json};

    use super::super::{Normalizer, Parsed};

    /// A line of the message `message` from `sender`, holding `block`.
    fn line(sender: Option<&str>, message: &str, block: &Value) -> String {
        json!({"type":"assistant","parent_tool_use_id":sender,"message":{"id":message,"content":[block]}}).to_string()
    }

    fn asks(id: &str) -> Value {
        json!({"type":"server_tool_use","id":id,"name":"advisor","input":{}})
    }

    fn answers(id: &str) -> Value {
        json!({"type":"advisor_tool_result","tool_use_id":id,"content":{"type":"advisor_redacted_result"}})
    }

    fn runs(id: &str) -> Value {
        json!({"type":"tool_use","id":id,"name":"Bash","input":{"command":"ls"}})
    }

    /// The calls each of `lines` showed were dropped, in order.
    fn dropped_by(lines: &[String]) -> Vec<Vec<String>> {
        let mut normalizer = Normalizer::default();
        let ids = |line: &String| normalizer.line(line).dropped.iter().map(|id| id.as_str().to_string()).collect();
        lines.iter().map(ids).collect()
    }

    #[test]
    fn a_call_beside_one_of_the_agent_s_own_is_dropped_when_the_next_message_begins_without_its_result() {
        // The order the CLI sent these in: the command's result comes before the advisor's call has finished.
        let result = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]}});
        let status = json!({"type":"system","subtype":"status","status":"requesting"});
        let lines = [
            line(None, "m1", &runs("t1")),
            result.to_string(),
            line(None, "m1", &asks("first")),
            status.to_string(),
            line(None, "m2", &asks("second")),
            line(None, "m2", &answers("second")),
            line(None, "m2", &runs("t2")),
        ];

        let dropped = dropped_by(&lines);

        assert_eq!(dropped[4], ["first"], "the next message asked again rather than answering");
        assert!(dropped.iter().enumerate().all(|(at, calls)| at == 4 || calls.is_empty()), "{dropped:?}");
    }

    #[test]
    fn a_line_that_shows_a_call_was_dropped_still_says_what_it_says() {
        let mut normalizer = Normalizer::default();
        normalizer.line(&line(None, "m1", &asks("first")));

        let read = normalizer.line(&line(None, "m2", &runs("t1")));

        let Parsed::Event(EventBody::Assistant { message_id, .. }) = read.parsed else { panic!("{:?}", read.parsed) };
        assert_eq!((read.dropped, message_id), (vec!["first".into()], Some("m2".into())));
    }

    #[test]
    fn a_call_answered_in_its_own_message_or_at_the_start_of_the_next_isn_t_dropped() {
        let same = [line(None, "m1", &asks("a")), line(None, "m1", &answers("a")), line(None, "m2", &runs("t"))];
        let next = [line(None, "m1", &asks("a")), line(None, "m2", &answers("a")), line(None, "m2", &runs("t"))];
        // Another tool the API runs, whose result this build reads as nothing, still names its call.
        let unread = json!({"type":"web_search_tool_result","tool_use_id":"a","content":[]});
        let other = [line(None, "m1", &asks("a")), line(None, "m1", &unread), line(None, "m2", &runs("t"))];

        for lines in [same, next, other] {
            assert!(dropped_by(&lines).iter().all(Vec::is_empty), "{lines:?}");
        }
    }

    #[test]
    fn a_call_its_turn_ended_on_isn_t_dropped_by_the_next_turn() {
        let ended = json!({"type":"result","subtype":"error_during_execution","is_error":true});
        let lines = [line(None, "m1", &asks("a")), ended.to_string(), line(None, "m2", &runs("t"))];

        assert!(dropped_by(&lines).iter().all(Vec::is_empty));
    }

    #[test]
    fn the_agent_s_calls_and_a_subagent_s_are_dropped_by_their_own_next_messages() {
        let ended = json!({"type":"result","subtype":"success"});
        let lines = [
            line(Some("agent"), "s1", &asks("its")),
            line(None, "m1", &asks("mine")),
            line(None, "m2", &runs("t1")),
            ended.to_string(),
            line(Some("agent"), "s2", &runs("t2")),
        ];

        let dropped = dropped_by(&lines);

        assert_eq!(dropped[2], ["mine"], "the agent's next message says nothing of the subagent's call");
        assert_eq!(dropped[4], ["its"], "and the subagent's, which worked on past the agent's turn, drops its own");
    }
}
