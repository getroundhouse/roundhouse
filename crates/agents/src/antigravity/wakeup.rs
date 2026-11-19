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
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::time::MissedTickBehavior;

use super::control;
use super::running::Shared;
use super::state::PromptOrigin;

const POLL_INTERVAL: Duration = Duration::from_millis(150);
/// How long an apparently ungated wakeup tool call (`read_file` when running without `--dangerously-skip-permissions`)
/// may sit without `agy` appending all `GENERIC` execution steps of that batch before concluding an out-of-workspace
/// path parked it in `SetPendingToolPermission`.
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
        pending: usize,
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

    fn feed_bytes(&mut self, bytes: &[u8], skip_permissions: bool, now: Instant) -> usize {
        let Some(end) = bytes.iter().rposition(|&b| b == b'\n').map(|idx| idx + 1) else {
            return 0;
        };
        self.offset = self.offset.saturating_add(u64::try_from(end).unwrap_or(u64::MAX));
        for line in bytes[..end].split(|&b| b == b'\n').filter(|line| !line.is_empty()) {
            if let Ok(step) = serde_json::from_slice::<Value>(line) {
                self.on_step(&step, skip_permissions, now);
            }
        }
        end
    }

    fn on_step(&mut self, step: &Value, skip_permissions: bool, now: Instant) {
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
            "GENERIC" => {
                if let Phase::RunningUngated { pending, .. } = &mut self.phase {
                    *pending = pending.saturating_sub(1);
                    if *pending == 0 {
                        self.phase = Phase::Waking;
                    }
                }
            }
            "PLANNER_RESPONSE" if status == "DONE" => {
                let tools = tool_names(step);
                if tools.is_empty() {
                    self.phase = Phase::Ready(DrainAction::FlushAndCancel);
                } else if tools.iter().any(|name| gates_on_interaction(name, skip_permissions)) {
                    self.phase = Phase::Ready(DrainAction::RunTurn);
                } else if skip_permissions {
                    self.phase = Phase::Waking;
                } else {
                    self.phase = Phase::RunningUngated { pending: tools.len(), since: now };
                }
            }
            _ => {}
        }
    }

    fn next_action(&self, now: Instant) -> Option<DrainAction> {
        match self.phase {
            Phase::Ready(action) => Some(action),
            Phase::RunningUngated { since, .. } if now.duration_since(since) >= UNGATED_PARK_GRACE => {
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

fn gates_on_interaction(tool: &str, skip_permissions: bool) -> bool {
    if skip_permissions {
        tool == "ask_question"
    } else {
        matches!(tool, "ask_question" | "run_command" | "edit_file" | "write_file")
    }
}

pub(super) async fn watch(shared: Arc<Shared>, child_gone: Arc<AtomicBool>) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut tracker: Option<(std::path::PathBuf, TranscriptWatch)> = None;
    while !shared.gone.load(Ordering::SeqCst) && !child_gone.load(Ordering::SeqCst) {
        interval.tick().await;
        let (transcript, in_turn, skip_permissions) = {
            let state = shared.state.lock();
            (state.transcript.clone(), state.in_turn(), state.skip_permissions)
        };
        let Some(cursor) = transcript else { continue };
        let path = cursor.path;
        let floor_offset = cursor.wakeup_floor;
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
            let read_start = watch_state.offset;
            let consumed = watch_state.feed_bytes(&bytes, skip_permissions, Instant::now());
            if consumed > 0 {
                let mut guard = shared.state.lock();
                let state = &mut *guard;
                let slice_end = read_start.saturating_add(u64::try_from(consumed).unwrap_or(u64::MAX));
                if let Some(cursor) =
                    state.transcript.as_mut().filter(|c| c.path == path && slice_end > c.async_agents_offset)
                {
                    let skip =
                        usize::try_from(cursor.async_agents_offset.saturating_sub(read_start)).unwrap_or(consumed);
                    if let Some(unread) = bytes.get(skip..consumed) {
                        let fed = state.async_agents.on_transcript_bytes(unread);
                        let next = read_start
                            .saturating_add(u64::try_from(skip).unwrap_or(u64::MAX))
                            .saturating_add(u64::try_from(fed).unwrap_or(u64::MAX));
                        cursor.async_agents_offset = cursor.async_agents_offset.max(next);
                    }
                }
            }
        }
        if let Some(action) = watch_state.next_action(Instant::now())
            && trigger_drain(&shared, action).await
        {
            watch_state.mark_draining();
        }
    }
}

pub(super) async fn read_since(path: &Path, offset: u64) -> Option<Vec<u8>> {
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
    fn wakeup_watch_handles_skip_permissions_mixed_batches_and_parked_plan_reads() {
        let t0 = Instant::now();
        let mut watch = TranscriptWatch::new(0);
        watch.feed_bytes(&line("USER_EXPLICIT", "USER_INPUT", "DONE", &[]), true, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["run_command"]), true, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &[]), true, t0);
        assert_eq!(watch.next_action(t0), None, "ordinary user turn must not trigger a wakeup drain");

        // Under --dangerously-skip-permissions (`skip_permissions = true`), `run_command` runs unasked without firing
        // `UNGATED_PARK_GRACE` mid-command, then flushes once the follow-up `PLANNER_RESPONSE` finishes.
        let partial = b"{\"source\":\"SYSTEM\",\"type\":\"SYSTEM_MESSAGE\",\"status\":\"DONE\"}";
        watch.feed_bytes(partial, true, t0);
        assert_eq!(watch.next_action(t0), None, "incomplete line without newline is held back");
        let mut complete = partial.to_vec();
        complete.push(b'\n');
        complete.extend_from_slice(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["run_command"]));
        watch.feed_bytes(&complete, true, t0);
        assert_eq!(watch.next_action(t0 + Duration::from_secs(5)), None, "unasked run_command must not drain early");
        watch.feed_bytes(&line("MODEL", "GENERIC", "DONE", &[]), true, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &[]), true, t0);
        assert_eq!(watch.next_action(t0), Some(DrainAction::FlushAndCancel));
        watch.mark_draining();

        // Mixed batch `[read_file, read_file, run_command]` in Plan mode (`skip_permissions = false`) where the first
        // ungated `read_file` appends its `GENERIC` step immediately must preserve `Ready(RunTurn)`.
        let mut batch = Vec::new();
        batch.extend_from_slice(&line("USER_EXPLICIT", "USER_INPUT", "DONE", &[]));
        batch.extend_from_slice(&line("SYSTEM", "SYSTEM_MESSAGE", "DONE", &[]));
        batch.extend_from_slice(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["read_file", "read_file", "run_command"]));
        batch.extend_from_slice(&line("MODEL", "GENERIC", "DONE", &[]));
        watch.feed_bytes(&batch, false, t0);
        assert_eq!(watch.next_action(t0), Some(DrainAction::RunTurn), "sibling GENERIC must not clobber RunTurn");

        // Mixed batch `[read_file, ask_question]` under `skip_permissions = true` also preserves `Ready(RunTurn)`.
        let mut ask_batch = Vec::new();
        ask_batch.extend_from_slice(&line("USER_EXPLICIT", "USER_INPUT", "DONE", &[]));
        ask_batch.extend_from_slice(&line("SYSTEM", "SYSTEM_MESSAGE", "DONE", &[]));
        ask_batch.extend_from_slice(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["read_file", "ask_question"]));
        ask_batch.extend_from_slice(&line("MODEL", "GENERIC", "DONE", &[]));
        watch.feed_bytes(&ask_batch, true, t0);
        assert_eq!(watch.next_action(t0), Some(DrainAction::RunTurn));

        // Two `read_file` calls in Plan mode where only one finishes keep `RunningUngated` armed for the parked second.
        watch.feed_bytes(&line("USER_EXPLICIT", "USER_INPUT", "DONE", &[]), false, t0);
        watch.feed_bytes(&line("SYSTEM", "SYSTEM_MESSAGE", "DONE", &[]), false, t0);
        watch.feed_bytes(&line("MODEL", "PLANNER_RESPONSE", "DONE", &["read_file", "read_file"]), false, t0);
        watch.feed_bytes(&line("MODEL", "GENERIC", "DONE", &[]), false, t0);
        assert_eq!(watch.next_action(t0), None);
        assert_eq!(watch.next_action(t0 + UNGATED_PARK_GRACE), Some(DrainAction::RunTurn));
    }
}
