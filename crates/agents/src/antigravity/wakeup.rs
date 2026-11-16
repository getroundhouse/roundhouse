//! Drains between-turn reactive wakeups from `agy` 1.5.9, which speaks only ACP 1.0 (`protocolVersion: 1`) and
//! attaches its trajectory update stream and permission context only while a `session/prompt` RPC is in flight.
//!
//! Workaround (`docs/reviews.md`): when a background command, schedule, workflow or subagent wakes `agy` between
//! turns, `agy` writes a `SYSTEM_MESSAGE` step and its `PLANNER_RESPONSE` to
//! `brain/<sessionId>/.system_generated/logs/transcript.jsonl` while buffering ACP updates and parking any permission
//! request in memory. Tailing that log by byte offset while idle lets the adapter open an `OwnTurn` with one internal
//! `session/prompt`, cancelling it on `user_message_chunk` before an extra model call when the wakeup has already
//! finished. Remove this module once `agy` adopts ACP v2's asynchronous session updates.

use std::io::SeekFrom;
use std::path::Path;
use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};

use protocol::model::PermissionMode;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::time::MissedTickBehavior;

use super::control;
use super::running::Shared;
use super::state::PromptOrigin;

const POLL_INTERVAL: Duration = Duration::from_millis(150);
/// How long an apparently ungated wakeup tool call (`read_file`, `edit_file`, `write_file` in `FullAccess`) may sit
/// without `agy` appending its `GENERIC` execution step before we conclude an out-of-workspace path parked it in
/// `SetPendingToolPermission`.
const UNGATED_PARK_GRACE: Duration = Duration::from_millis(450);
const DRAIN_PROMPT: &str =
    "Continue from the completed background task if anything remains, otherwise briefly confirm.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DrainAction {
    /// Wakeup already finished its prose; flush buffered updates and cancel on `user_message_chunk`.
    FlushAndCancel,
    /// Wakeup parked on an interaction-gated tool call; keep `session/prompt` open so the tool and follow-up run.
    RunTurn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Waking,
    Ready(DrainAction),
    RunningUngated {
        since: Instant,
    },
    Draining,
}

#[derive(Default)]
struct TranscriptWatch {
    offset: u64,
    phase: Phase,
}

impl TranscriptWatch {
    fn new(offset: u64) -> Self {
        Self { offset, phase: Phase::Idle }
    }

    fn feed_bytes(&mut self, bytes: &[u8], mode: PermissionMode, now: Instant) {
        let Some(end) = bytes.iter().rposition(|&b| b == b'\n').map(|idx| idx + 1) else {
            return;
        };
        self.offset = self.offset.saturating_add(u64::try_from(end).unwrap_or(u64::MAX));
        for line in bytes[..end].split(|&b| b == b'\n').filter(|line| !line.is_empty()) {
            if let Ok(step) = serde_json::from_slice::<Value>(line) {
                self.on_step(&step, mode, now);
            }
        }
    }

    fn on_step(&mut self, step: &Value, mode: PermissionMode, now: Instant) {
        let source = step.get("source").and_then(Value::as_str).unwrap_or_default();
        let kind = step.get("type").and_then(Value::as_str).unwrap_or_default();
        let status = step.get("status").and_then(Value::as_str).unwrap_or_default();
        if kind == "USER_INPUT" {
            self.phase = Phase::Idle;
            return;
        }
        if source == "SYSTEM" && kind == "SYSTEM_MESSAGE" {
            self.phase = Phase::Waking;
            return;
        }
        if matches!(self.phase, Phase::Idle | Phase::Draining) {
            return;
        }
        match kind {
            "ERROR_MESSAGE" => self.phase = Phase::Ready(DrainAction::FlushAndCancel),
            "GENERIC" => self.phase = Phase::Waking,
            "PLANNER_RESPONSE" if status == "DONE" => {
                let tools = tool_names(step);
                if tools.is_empty() {
                    self.phase = Phase::Ready(DrainAction::FlushAndCancel);
                } else if mode == PermissionMode::Plan || tools.iter().any(|name| gates_in_full_access(name)) {
                    self.phase = Phase::Ready(DrainAction::RunTurn);
                } else {
                    self.phase = Phase::RunningUngated { since: now };
                }
            }
            _ => {}
        }
    }

    fn next_action(&self, now: Instant) -> Option<DrainAction> {
        match self.phase {
            Phase::Ready(action) => Some(action),
            Phase::RunningUngated { since } if now.duration_since(since) >= UNGATED_PARK_GRACE => {
                Some(DrainAction::RunTurn)
            }
            Phase::Idle | Phase::Waking | Phase::RunningUngated { .. } | Phase::Draining => None,
        }
    }

    fn mark_draining(&mut self) {
        self.phase = Phase::Draining;
    }
}

fn tool_names(step: &Value) -> Vec<&str> {
    step.get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|call| call.get("name").and_then(Value::as_str))
        .collect()
}

fn gates_in_full_access(tool: &str) -> bool {
    matches!(tool, "run_command" | "ask_question")
}

pub(super) async fn watch(shared: Arc<Shared>) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut tracker: Option<(std::path::PathBuf, TranscriptWatch)> = None;
    while !shared.gone.load(Ordering::SeqCst) {
        interval.tick().await;
        let (transcript, in_turn, mode) = {
            let state = shared.state.lock();
            (state.transcript.clone(), state.in_turn(), state.mode)
        };
        let Some((path, floor_offset)) = transcript else { continue };
        let (watched_path, watch_state) =
            tracker.get_or_insert_with(|| (path.clone(), TranscriptWatch::new(floor_offset)));
        if *watched_path != path {
            watched_path.clone_from(&path);
            *watch_state = TranscriptWatch::new(floor_offset);
        } else if watch_state.offset < floor_offset {
            *watch_state = TranscriptWatch::new(floor_offset);
        }
        if in_turn {
            continue;
        }
        if let Some(bytes) = read_since(&path, watch_state.offset).await {
            watch_state.feed_bytes(&bytes, mode, Instant::now());
        }
        if let Some(action) = watch_state.next_action(Instant::now())
            && trigger_drain(&shared, action).await
        {
            watch_state.mark_draining();
        }
    }
}

async fn read_since(path: &Path, offset: u64) -> Option<Vec<u8>> {
    let len = tokio::fs::metadata(path).await.ok()?.len();
    if len <= offset {
        return None;
    }
    let mut file = tokio::fs::File::open(path).await.ok()?;
    file.seek(SeekFrom::Start(offset)).await.ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await.ok()?;
    (!buf.is_empty()).then_some(buf)
}

async fn trigger_drain(shared: &Arc<Shared>, action: DrainAction) -> bool {
    let _ordered = shared.controls.lock().await;
    if shared.gone.load(Ordering::SeqCst) {
        return false;
    }
    let keep_open = action == DrainAction::RunTurn;
    let origin = PromptOrigin::WakeupDrain { keep_open, started_own_turn: false, self_cancelled: false };
    let blocks = vec![json!({ "type": "text", "text": DRAIN_PROMPT })];
    control::send_drain_prompt(shared, origin, blocks).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(source: &str, kind: &str, status: &str, tools: &[&str]) -> Vec<u8> {
        let calls: Vec<Value> = tools.iter().map(|name| json!({ "name": name })).collect();
        let mut s = json!({ "source": source, "type": kind, "status": status, "tool_calls": calls }).to_string();
        s.push('\n');
        s.into_bytes()
    }

    #[test]
    fn wakeup_watch_distinguishes_normal_turns_plain_wakeups_gated_tools_and_parked_reads() {
        let t0 = Instant::now();
        let mut watch = TranscriptWatch::new(0);
        watch.feed_bytes(&line("USER_EXPLICIT", "USER_INPUT", "DONE", &[]), PermissionMode::FullAccess, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["run_command"]), PermissionMode::FullAccess, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &[]), PermissionMode::FullAccess, t0);
        assert_eq!(watch.next_action(t0), None, "ordinary user turn must not trigger a wakeup drain");

        let partial = b"{\"source\":\"SYSTEM\",\"type\":\"SYSTEM_MESSAGE\",\"status\":\"DONE\"}";
        watch.feed_bytes(partial, PermissionMode::FullAccess, t0);
        assert_eq!(watch.next_action(t0), None, "incomplete line without newline is held back");

        let mut complete = partial.to_vec();
        complete.push(b'\n');
        complete.extend_from_slice(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["read_file"]));
        watch.feed_bytes(&complete, PermissionMode::FullAccess, t0);
        assert_eq!(watch.next_action(t0), None, "ungated read_file waits for GENERIC step first");

        watch.feed_bytes(&line("MODEL", "GENERIC", "DONE", &[]), PermissionMode::FullAccess, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &[]), PermissionMode::FullAccess, t0);
        assert_eq!(watch.next_action(t0), Some(DrainAction::FlushAndCancel));
        watch.mark_draining();
        assert_eq!(watch.next_action(t0), None, "Draining state prevents duplicate drain RPCs");

        watch.feed_bytes(&line("USER_EXPLICIT", "USER_INPUT", "DONE", &[]), PermissionMode::FullAccess, t0);
        watch.feed_bytes(&line("SYSTEM", "SYSTEM_MESSAGE", "DONE", &[]), PermissionMode::FullAccess, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["run_command"]), PermissionMode::FullAccess, t0);
        assert_eq!(watch.next_action(t0), Some(DrainAction::RunTurn));

        watch.feed_bytes(&line("USER_EXPLICIT", "USER_INPUT", "DONE", &[]), PermissionMode::FullAccess, t0);
        watch.feed_bytes(&line("SYSTEM", "SYSTEM_MESSAGE", "DONE", &[]), PermissionMode::FullAccess, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["read_file"]), PermissionMode::FullAccess, t0);
        assert_eq!(watch.next_action(t0 + UNGATED_PARK_GRACE), Some(DrainAction::RunTurn));
    }
}
