use std::collections::HashSet;

use protocol::capability::calls::Call;
use serde_json::Value;

use crate::acp::{ToolCallWire, ToolStatusWire};

const CREATED_SUBAGENT_MARKER: &str = "Created the following subagent:";
const SUBAGENT_PREFIX: &str = "Subagent ";
const SUBAGENT_IDLE_SUFFIX: &str = " has gone idle.";

/// Tracks background subagents (`invoke_subagent`) and swarm workers (`swarm`) spawned by the current `agy` child
/// process so plan-holding waits for same-turn spawns and wakeup drains skip `session/cancel` only while an async
/// agent is still live.
#[derive(Debug, Default)]
pub(super) struct AsyncAgents {
    spawned_this_turn: bool,
    known_subagents: HashSet<String>,
    unnamed_subagents: usize,
    finished_subagents: HashSet<String>,
    swarm_workers: HashSet<String>,
}

impl AsyncAgents {
    pub(super) fn spawned_this_turn(&self) -> bool {
        self.spawned_this_turn && self.has_live()
    }

    pub(super) fn has_live(&self) -> bool {
        self.has_live_subagents() || !self.swarm_workers.is_empty()
    }

    pub(super) fn has_live_subagents(&self) -> bool {
        !self.known_subagents.is_empty() || self.unnamed_subagents > 0
    }

    pub(super) fn on_turn_settled(&mut self) {
        self.spawned_this_turn = false;
    }

    pub(super) fn clear(&mut self) {
        self.spawned_this_turn = false;
        self.known_subagents.clear();
        self.unnamed_subagents = 0;
        self.finished_subagents.clear();
        self.swarm_workers.clear();
    }

    pub(super) fn on_tool_completed(&mut self, call: &Call, wire: &ToolCallWire) {
        if wire.status != Some(ToolStatusWire::Completed) {
            return;
        }
        match call {
            Call::Subagent(_) => {
                self.spawned_this_turn = true;
                if let Some(id) = extract_subagent_id_from_wire(wire) {
                    self.finished_subagents.remove(id);
                    self.known_subagents.insert(id.to_owned());
                } else {
                    self.unnamed_subagents += 1;
                }
            }
            Call::StopSubagent(stop) => self.subagent_ended(stop.subagent.as_str()),
            Call::MessageSubagent(msg) => self.subagent_messaged(msg.subagent.as_str()),
            Call::Other { tool, input } if tool == "manage_subagents" => self.on_manage_subagents(input),
            Call::Other { tool, input } if tool == "swarm" => self.on_swarm(input),
            Call::Command(_)
            | Call::WriteInput(_)
            | Call::Read(_)
            | Call::Write(_)
            | Call::Edit(_)
            | Call::Patch(_)
            | Call::NotebookEdit(_)
            | Call::Search(_)
            | Call::FindFiles(_)
            | Call::Fetch(_)
            | Call::WebSearch(_)
            | Call::FindInPage(_)
            | Call::Todos(_)
            | Call::WaitForSubagents(_)
            | Call::ViewImage(_)
            | Call::GenerateImage(_)
            | Call::Skill(_)
            | Call::Lsp(_)
            | Call::Advisor
            | Call::Sleep(_)
            | Call::Monitor(_)
            | Call::Schedule(_)
            | Call::Workflow(_)
            | Call::Questions
            | Call::Plan
            | Call::Mcp { .. }
            | Call::Other { .. }
            | Call::Unknown => {}
        }
    }

    /// Parses every complete JSONL line in `bytes` and returns the byte count consumed through the last `\n` (`0` if
    /// `bytes` has no newline yet, leaving any incomplete trailing line for the next read).
    pub(super) fn on_transcript_bytes(&mut self, bytes: &[u8]) -> usize {
        let Some(end) = bytes.iter().rposition(|&b| b == b'\n').map(|idx| idx + 1) else {
            return 0;
        };
        for line in bytes[..end].split(|&b| b == b'\n').filter(|line| !line.is_empty()) {
            if let Ok(step) = serde_json::from_slice::<Value>(line) {
                self.on_transcript_step(&step);
            }
        }
        end
    }

    pub(super) fn on_transcript_step(&mut self, step: &Value) {
        let kind = step.get("type").and_then(Value::as_str).unwrap_or_default();
        let content = step.get("content").and_then(Value::as_str).unwrap_or_default();
        if kind == "GENERIC" {
            if let Some(id) = parse_created_subagent_id(content) {
                self.register_subagent_id(id);
            }
        } else if kind == "SYSTEM_MESSAGE" {
            for id in parse_idle_subagent_ids(content) {
                self.subagent_ended(id);
            }
        }
    }

    fn on_manage_subagents(&mut self, args: &Value) {
        match args.get("Action").and_then(Value::as_str) {
            Some("kill_all") => {
                self.finished_subagents.extend(self.known_subagents.drain());
                self.unnamed_subagents = 0;
            }
            Some("kill") => {
                if let Some(ids) = args.get("ConversationIds").and_then(Value::as_array) {
                    for id in ids.iter().filter_map(Value::as_str) {
                        self.subagent_ended(id);
                    }
                }
            }
            _ => {}
        }
    }

    fn on_swarm(&mut self, args: &Value) {
        match args.get("action").and_then(Value::as_str) {
            Some("add") => {
                let Some(base) = args.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()) else {
                    return;
                };
                let count = args
                    .get("tasks")
                    .and_then(Value::as_array)
                    .filter(|tasks| !tasks.is_empty())
                    .map(Vec::len)
                    .or_else(|| {
                        args.get("n").and_then(Value::as_u64).filter(|&n| n > 1).and_then(|n| usize::try_from(n).ok())
                    });
                if let Some(count) = count {
                    for idx in 1..=count {
                        self.swarm_workers.insert(format!("{base}-{idx}"));
                    }
                } else {
                    self.swarm_workers.insert(base.to_owned());
                }
                self.spawned_this_turn = true;
            }
            Some("kill") => {
                if let Some(name) = args.get("name").and_then(Value::as_str) {
                    self.swarm_workers.remove(name);
                }
            }
            _ => {}
        }
    }

    fn register_subagent_id(&mut self, id: &str) {
        if self.finished_subagents.contains(id) {
            return;
        }
        if self.known_subagents.insert(id.to_owned()) {
            self.unnamed_subagents = self.unnamed_subagents.saturating_sub(1);
        }
    }

    fn subagent_ended(&mut self, id: &str) {
        if self.finished_subagents.insert(id.to_owned()) && !self.known_subagents.remove(id) {
            self.unnamed_subagents = self.unnamed_subagents.saturating_sub(1);
        }
    }

    fn subagent_messaged(&mut self, id: &str) {
        if self.finished_subagents.remove(id) {
            self.known_subagents.insert(id.to_owned());
        }
    }
}

fn extract_subagent_id_from_wire(wire: &ToolCallWire) -> Option<&str> {
    if let Some(id) = wire
        .raw_output
        .as_ref()
        .and_then(|raw| raw.get("conversationId").and_then(Value::as_str).filter(|s| !s.is_empty()))
    {
        return Some(id);
    }
    wire.raw_output
        .as_ref()
        .and_then(Value::as_str)
        .and_then(parse_created_subagent_id)
        .or_else(|| wire.content_text.as_deref().and_then(parse_created_subagent_id))
}

fn parse_created_subagent_id(content: &str) -> Option<&str> {
    let mut rest = content.trim_start();
    while let Some(after) = rest.strip_prefix("Created At:").or_else(|| rest.strip_prefix("Completed At:")) {
        rest = after.split_once('\n').map_or("", |(_, tail)| tail).trim_start();
    }
    let after_marker = rest.strip_prefix(CREATED_SUBAGENT_MARKER)?;
    let (_, after_key) = after_marker.split_once("\"conversationId\"")?;
    let after_colon = after_key.trim_start().strip_prefix(':')?.trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let (id, _) = after_quote.split_once('"')?;
    (!id.is_empty()).then_some(id)
}

fn parse_idle_subagent_ids(content: &str) -> Vec<&str> {
    let mut ids = Vec::new();
    let mut rest = content;
    while let Some(pos) = rest.find(SUBAGENT_PREFIX) {
        rest = &rest[pos + SUBAGENT_PREFIX.len()..];
        if let Some(end) = rest.find(SUBAGENT_IDLE_SUFFIX) {
            let candidate = &rest[..end];
            if !candidate.is_empty() && !candidate.chars().any(char::is_whitespace) {
                ids.push(candidate);
                rest = &rest[end + SUBAGENT_IDLE_SUFFIX.len()..];
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::antigravity::calls::translate_call;

    fn completed_wire(name: &str, args: Value) -> (Call, ToolCallWire) {
        let wire = ToolCallWire {
            tool_call_id: "c".into(),
            status: Some(ToolStatusWire::Completed),
            name: Some(name.into()),
            args: Some(args),
            ..ToolCallWire::default()
        };
        let call = translate_call(&wire);
        (call, wire)
    }

    #[test]
    fn subagents_clear_on_idle_kill_and_reopen_on_send_message() {
        let mut tracked = AsyncAgents::default();

        // Failed invoke_subagent does not mark anything live.
        let mut failed_wire = completed_wire("invoke_subagent", json!({ "Prompt": "x" })).1;
        failed_wire.status = Some(ToolStatusWire::Failed);
        tracked.on_tool_completed(&translate_call(&failed_wire), &failed_wire);
        assert!(!tracked.has_live());
        assert!(!tracked.spawned_this_turn());

        // Spawn two subagents without rawOutput (matching agy 1.5.12 ACP wire).
        let (call, wire) = completed_wire("invoke_subagent", json!({ "Prompt": "a" }));
        tracked.on_tool_completed(&call, &wire);
        tracked.on_tool_completed(&call, &wire);
        assert!(tracked.spawned_this_turn());
        assert!(tracked.has_live());
        tracked.on_turn_settled();
        assert!(!tracked.spawned_this_turn());
        assert!(tracked.has_live());

        // Transcript reveals one ID; first going idle leaves the second live.
        tracked.on_transcript_step(&json!({
            "type": "GENERIC",
            "content": "Created the following subagent:\n{\n  \"conversationId\":  \"sub-1\"\n}"
        }));
        tracked.on_transcript_step(&json!({
            "type": "SYSTEM_MESSAGE",
            "content": "[Message] sender=system content=Subagent sub-1 has gone idle. Done."
        }));
        assert!(tracked.has_live());

        // Second going idle clears all live subagents.
        tracked.on_transcript_step(&json!({
            "type": "SYSTEM_MESSAGE",
            "content": "[Message] sender=system content=Subagent sub-2 has gone idle. Done."
        }));
        assert!(!tracked.has_live());

        // Messaging an idle subagent re-marks it as live until killed.
        let (msg_call, msg_wire) = completed_wire("send_message", json!({ "Recipient": "sub-1", "Message": "go" }));
        tracked.on_tool_completed(&msg_call, &msg_wire);
        assert!(tracked.has_live());

        let (kill_call, kill_wire) =
            completed_wire("manage_subagents", json!({ "Action": "kill", "ConversationIds": ["sub-1"] }));
        tracked.on_tool_completed(&kill_call, &kill_wire);
        assert!(!tracked.has_live());

        // kill_all clears remaining unnamed subagents.
        tracked.on_tool_completed(&call, &wire);
        assert!(tracked.has_live());
        let (kill_all_call, kill_all_wire) = completed_wire("manage_subagents", json!({ "Action": "kill_all" }));
        tracked.on_tool_completed(&kill_all_call, &kill_all_wire);
        assert!(!tracked.has_live());

        // Non-anchored marker inside file/command output must not falsely latch a phantom subagent, and
        // `on_transcript_bytes` must report only complete lines consumed through the final newline.
        let source_line = json!({
            "type": "GENERIC",
            "content": "const M: &str = \"Created the following subagent:\"; \"conversationId\": \"phantom\""
        })
        .to_string();
        let bytes = format!("{source_line}\n{{\"type\":\"SYSTEM_MESSAGE\"");
        assert_eq!(tracked.on_transcript_bytes(bytes.as_bytes()), source_line.len() + 1);
        assert!(!tracked.has_live());
    }

    #[test]
    fn swarm_tracks_added_workers_and_ignores_read_only_actions() {
        let mut tracked = AsyncAgents::default();

        let (status_call, status_wire) = completed_wire("swarm", json!({ "action": "status" }));
        tracked.on_tool_completed(&status_call, &status_wire);
        assert!(!tracked.has_live());
        assert!(!tracked.spawned_this_turn());

        let (add_call, add_wire) = completed_wire("swarm", json!({ "action": "add", "name": "worker", "n": 2 }));
        tracked.on_tool_completed(&add_call, &add_wire);
        assert!(tracked.spawned_this_turn());
        assert!(tracked.has_live());
        tracked.on_turn_settled();

        let (kill1_call, kill1_wire) = completed_wire("swarm", json!({ "action": "kill", "name": "worker-1" }));
        tracked.on_tool_completed(&kill1_call, &kill1_wire);
        assert!(tracked.has_live());

        let (kill2_call, kill2_wire) = completed_wire("swarm", json!({ "action": "kill", "name": "worker-2" }));
        tracked.on_tool_completed(&kill2_call, &kill2_wire);
        assert!(!tracked.has_live());
    }
}
