//! Plain text for a terminal. This is a debugging view, not the product's transcript.

use std::collections::HashMap;
use std::io::Write;

use client_core::calls::{ToolCall, first_line};
use protocol::attachment::Attachment;
use protocol::event::{
    AgentTask, CompactionOutcome, ContentBlock, EventBody, Question, SessionEvent, SubagentOutcome, TurnOutcome,
};
use protocol::model::{ApprovalId, MessageId, PermissionMode, Seq, TaskId, ToolUseId};
use protocol::tail::{DeltaChange, StreamDelta, Tail};
use serde_json::Value;

mod approval;

/// How much of a tool's result to show. The full text stays in the log.
const RESULT_PREVIEW_LINES: usize = 6;

#[derive(Default)]
pub(crate) struct Printer {
    /// How many bytes of each block of text have been printed as it streamed, so a tail that replaces what's
    /// held, as when following starts again, and then the finished block, only print what's new.
    printed: HashMap<(MessageId, u32), usize>,
    mid_line: bool,
    /// What the newest list of background tasks printed as, so one that says the same isn't printed again. The agent
    /// lists a monitor's task twice, since it only learns it's a monitor after the first.
    background: Option<String>,
    /// What each subagent that hasn't ended was started for, so its end can say which it was.
    subagents: HashMap<ToolUseId, String>,
    /// The questions each open approval asks, so what was answered can be said by question.
    asked: HashMap<ApprovalId, Vec<Question>>,
}

impl Printer {
    fn turn_ended(&mut self, seq: Seq, outcome: TurnOutcome, duration_ms: u64, cost: f64, errors: &[String]) {
        let what = match outcome {
            TurnOutcome::Success => "turn finished",
            TurnOutcome::Cancelled => "turn cancelled",
            TurnOutcome::Error => "turn failed",
            TurnOutcome::Unknown => "turn ended",
        };
        let seconds = std::time::Duration::from_millis(duration_ms).as_secs_f64();
        self.line(seq, &format!("{what} in {seconds:.1}s, ${cost:.4}"));
        for error in errors {
            self.line(seq, &format!("  {error}"));
        }
    }

    /// What was typed, and the names of the files it carries.
    fn prompt(&mut self, seq: Seq, text: &str, attachments: &[Attachment]) {
        let names: Vec<&str> = attachments.iter().map(|a| a.name.as_str()).collect();
        let carried = if names.is_empty() { String::new() } else { format!(" [attached: {}]", names.join(", ")) };
        self.line(seq, &format!("you: {text}{carried}"));
    }

    /// What the agent says it's running as, with `?` for what it left out.
    fn ready(&mut self, seq: Seq, model: Option<&str>, permission_mode: Option<PermissionMode>, cwd: Option<&str>) {
        let mode = permission_mode.map_or("?", crate::session::mode_name);
        let (model, cwd) = (model.unwrap_or("?"), cwd.unwrap_or("?"));
        self.line(seq, &format!("agent ready: model {model}, permissions {mode}, in {cwd}"));
    }

    pub(crate) fn event(&mut self, event: &SessionEvent) {
        match &event.body {
            EventBody::UserPrompt { prompt, .. } => self.prompt(event.seq, &prompt.text, &prompt.files),
            EventBody::AgentStarted { pid, resumed, .. } => {
                let how = if *resumed { "resumed" } else { "started" };
                self.line(event.seq, &format!("agent {how}, pid {pid}"));
            }
            EventBody::AgentInit { model, permission_mode, cwd, .. } => {
                self.ready(event.seq, model.as_deref(), *permission_mode, cwd.as_deref());
            }
            EventBody::Assistant { message_id, blocks, first_index, .. } => {
                for (position, block) in blocks.iter().enumerate() {
                    match (block, self.streamed(message_id.as_ref(), *first_index, position)) {
                        (ContentBlock::Text { text, .. }, Some(printed)) => self.rest(text, printed),
                        _ => self.block(event.seq, block),
                    }
                }
            }
            EventBody::AgentUser { blocks, .. } => {
                for block in blocks {
                    self.block(event.seq, block);
                }
            }
            EventBody::TurnEnded { outcome, duration_ms, cost_usd, errors, .. } => {
                // Whatever was still streaming, such as when the turn was cancelled, won't finish.
                self.printed.clear();
                self.turn_ended(event.seq, *outcome, duration_ms.unwrap_or(0), cost_usd.unwrap_or(0.0), errors);
            }
            EventBody::ApprovalRequested { .. } => self.approval_requested(event),
            EventBody::ApprovalResolved { .. } => self.approval_resolved(event),
            EventBody::AgentExited { .. } => self.exited(event),
            EventBody::Notice { level, message } => self.line(event.seq, &format!("{level:?}: {message}")),
            EventBody::AgentMode { permission_mode } => {
                self.line(event.seq, &format!("approval mode: {}", crate::session::mode_name(*permission_mode)));
            }
            EventBody::AgentTasks { tasks, stopped } => self.background(event.seq, tasks, stopped),
            EventBody::SubagentStarted { description, call, .. } => {
                self.subagent(event.seq, description.as_deref(), call);
            }
            EventBody::SubagentEnded { call, outcome, .. } => self.subagent_ended(event.seq, call, *outcome),
            EventBody::PromptDropped { prompt } => {
                self.line(event.seq, &format!("the agent stopped before taking what was asked at {prompt}"));
            }
            EventBody::ToolCallDropped { .. } => self.line(event.seq, "the agent went on without a call's result"),
            EventBody::CompactionAsked { keeping } => self.line(event.seq, &asked_to_compact(keeping.as_deref())),
            EventBody::AgentCompacting { written_chars } => self.line(event.seq, &compacting(*written_chars)),
            EventBody::AgentCompacted { outcome, .. } => self.line(event.seq, &compacted(outcome)),
            EventBody::CacheExpired { idle_ms, written_tokens, .. } => {
                self.line(event.seq, &cache_expired(*idle_ms, *written_tokens));
            }
            EventBody::PromptTaken { .. }
            | EventBody::AgentState { .. }
            | EventBody::AgentLimits { .. }
            | EventBody::AgentOther { .. }
            | EventBody::Unknown => {}
        }
    }

    /// That the agent exited, how, and the last of what it wrote to stderr.
    fn exited(&mut self, event: &SessionEvent) {
        let EventBody::AgentExited { reason, code, signal, stderr_tail } = &event.body else { return };
        self.line(event.seq, &format!("agent exited: {reason:?}, code {code:?}, signal {signal:?}"));
        for line in stderr_tail {
            self.line(event.seq, &format!("  {line}"));
        }
    }

    /// That the agent started a subagent, by what it's for, or failing that its call.
    fn subagent(&mut self, seq: u64, description: Option<&str>, call: &ToolUseId) {
        let what = description.unwrap_or(call.as_str());
        self.line(seq, &format!("started a subagent: {what}"));
        self.subagents.insert(call.clone(), what.to_string());
    }

    fn subagent_ended(&mut self, seq: u64, call: &ToolUseId, outcome: SubagentOutcome) {
        let ended = self.ending(call, outcome);
        self.line(seq, &ended);
    }

    /// How the subagent `call` started ended, named as its start was, or by the call if this didn't see it start.
    fn ending(&mut self, call: &ToolUseId, outcome: SubagentOutcome) -> String {
        let what = self.subagents.remove(call).unwrap_or_else(|| call.as_str().to_string());
        let how = match outcome {
            SubagentOutcome::Finished => "finished",
            SubagentOutcome::Failed => "failed",
            SubagentOutcome::Stopped => "stopped",
            SubagentOutcome::Unknown => "ended",
        };
        format!("a subagent {how}: {what}")
    }

    fn background(&mut self, seq: u64, tasks: &[AgentTask], stopped: &[TaskId]) {
        if let Some(running) = self.newly_running(tasks) {
            self.line(seq, &running);
        }
        if !stopped.is_empty() {
            let stopped: Vec<&str> = stopped.iter().map(TaskId::as_str).collect();
            self.line(seq, &format!("  stopped: {}", stopped.join(", ")));
        }
    }

    /// The line for a list of background tasks, unless it says what the last one did.
    fn newly_running(&mut self, tasks: &[AgentTask]) -> Option<String> {
        let what: Vec<&str> =
            tasks.iter().map(|task| task.description.as_deref().unwrap_or(task.id.as_str())).collect();
        let running = if what.is_empty() { "nothing".to_string() } else { what.join(", ") };
        let line = format!("running in the background: {running}");
        (self.background.as_ref() != Some(&line)).then(|| self.background.insert(line).clone())
    }

    fn block(&mut self, seq: u64, block: &ContentBlock) {
        match block {
            ContentBlock::Text { text, .. } => self.line(seq, text),
            ContentBlock::Thinking { text } if !text.is_empty() => {
                self.line(seq, &format!("(thinking) {}", first_line(text)));
            }
            ContentBlock::ToolUse { name, input, .. } => self.line(seq, &format!("tool {}", called(name, input))),
            ContentBlock::ToolResult { encrypted: true, .. } => {
                self.line(seq, "tool result: encrypted, for the model alone");
            }
            ContentBlock::ToolResult { is_error, text, images, .. } => {
                let label = if *is_error { "tool error" } else { "tool result" };
                let lines: Vec<&str> = text.lines().collect();
                let first = lines.first().copied().unwrap_or("");
                let shown = match images.len() {
                    0 => String::new(),
                    1 => "[1 image]".to_string(),
                    n => format!("[{n} images]"),
                };
                let gap = if first.is_empty() || shown.is_empty() { "" } else { " " };
                self.line(seq, &format!("{label}: {first}{gap}{shown}"));
                for line in lines.iter().skip(1).take(RESULT_PREVIEW_LINES - 1) {
                    self.line(seq, &format!("  {line}"));
                }
                if lines.len() > RESULT_PREVIEW_LINES {
                    self.line(seq, &format!("  ... {} more lines", lines.len() - RESULT_PREVIEW_LINES));
                }
            }
            ContentBlock::Thinking { .. } | ContentBlock::Image { .. } | ContentBlock::Unknown => {}
        }
    }

    /// How much of a finished block was printed as it streamed, which then needn't be remembered.
    fn streamed(&mut self, message: Option<&MessageId>, first_index: Option<u32>, position: usize) -> Option<usize> {
        let index = first_index?.checked_add(u32::try_from(position).ok()?)?;
        self.printed.remove(&(message?.clone(), index))
    }

    /// The end of a block that was printed as it streamed, such as what streamed while following had fallen
    /// behind.
    fn rest(&mut self, text: &str, printed: usize) {
        if let Some(rest) = text.get(printed..).filter(|rest| !rest.is_empty()) {
            self.mid_line = true;
            print!("{rest}");
        }
        self.end_line();
    }

    /// Text is printed as it arrives, and everything else waits for the finished block.
    pub(crate) fn delta(&mut self, delta: &StreamDelta) {
        if let DeltaChange::Text { index, text } = &delta.change {
            *self.printed.entry((delta.message_id.clone(), *index)).or_default() += text.len();
            self.mid_line = true;
            print!("{text}");
            std::io::stdout().flush().ok();
        }
    }

    pub(crate) fn tail(&mut self, tail: &Tail) {
        for message in &tail.messages {
            for block in &message.blocks {
                if block.kind != protocol::tail::BlockKind::Text {
                    continue;
                }
                let printed = self.printed.entry((message.message_id.clone(), block.index)).or_default();
                let Some(new) = block.text.get(*printed..).filter(|new| !new.is_empty()) else { continue };
                *printed = block.text.len();
                self.mid_line = true;
                print!("{new}");
            }
        }
        std::io::stdout().flush().ok();
    }

    fn end_line(&mut self) {
        if std::mem::take(&mut self.mid_line) {
            println!();
        }
    }

    fn line(&mut self, seq: u64, text: &str) {
        self.end_line();
        println!("[{seq}] {text}");
    }
}

/// A call to the tool `name`, with what it's about, as every client says it, if it says anything.
fn called(name: &str, input: &Value) -> String {
    about(name, ToolCall::read(name, input).summary())
}

/// The tool `name`, then `summary` after a colon, if there's one. Such as the advisor, which takes no input, a call can
/// have nothing to say.
fn about(name: &str, summary: Option<String>) -> String {
    summary.map_or_else(|| name.to_string(), |summary| format!("{name}: {summary}"))
}

/// That someone asked the agent to compact its conversation, and what to keep if they said.
fn asked_to_compact(keeping: Option<&str>) -> String {
    keeping.map_or_else(|| "asked to compact".to_string(), |keeping| format!("asked to compact, keeping: {keeping}"))
}

/// That the agent is compacting, with how much of the summary it has written, since it says so every 30 s.
fn compacting(written_chars: Option<u64>) -> String {
    let written = written_chars.map_or_else(String::new, |written| format!(", {written} characters written"));
    format!("compacting the conversation{written}")
}

/// That a prompt found the cache expired, how long the chat had been idle, and how much it wrote again.
fn cache_expired(idle_ms: u64, written_tokens: u64) -> String {
    let idle_min = idle_ms / 60_000;
    format!("the cache had expired after {idle_min} min idle, so {written_tokens} tokens were written to it again")
}

/// What compacting did, or why it couldn't.
fn compacted(outcome: &CompactionOutcome) -> String {
    match outcome {
        CompactionOutcome::Compacted { before_tokens: Some(before), after_tokens: Some(after), .. } => {
            format!("compacted the conversation from {before} tokens to {after}")
        }
        CompactionOutcome::Compacted { .. } | CompactionOutcome::Unknown => "compacted the conversation".to_string(),
        CompactionOutcome::Failed { error: Some(error) } => format!("couldn't compact the conversation: {error}"),
        CompactionOutcome::Failed { error: None } => "couldn't compact the conversation".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::{AgentTask, SubagentOutcome, TaskKind};
    use serde_json::json;

    use super::{Printer, called};

    fn task(id: &str, kind: TaskKind) -> AgentTask {
        AgentTask { id: id.into(), description: Some(format!("Do {id}")), kind, call: None }
    }

    #[test]
    fn a_list_of_background_tasks_said_again_with_a_kind_changed_prints_once() {
        let mut printer = Printer::default();
        let listed = [task("a", TaskKind::Command), task("b", TaskKind::Command)];
        let again = [task("a", TaskKind::Command), task("b", TaskKind::Monitor)];

        assert_eq!(printer.newly_running(&listed).as_deref(), Some("running in the background: Do a, Do b"));
        assert_eq!(printer.newly_running(&again), None);
        assert_eq!(printer.newly_running(&listed[..1]).as_deref(), Some("running in the background: Do a"));
        assert_eq!(printer.newly_running(&[]).as_deref(), Some("running in the background: nothing"));
        assert_eq!(printer.newly_running(&listed[..1]).as_deref(), Some("running in the background: Do a"));
    }

    #[test]
    fn a_call_says_what_it_s_about_as_every_client_does() {
        assert_eq!(called("Bash", &json!({ "command": "\ncargo test\n--workspace" })), "Bash: cargo test");
        assert_eq!(called("Read", &json!({ "file_path": "src/a.rs" })), "Read: src/a.rs");
        assert_eq!(called("mcp__github__list", &json!({ "labels": ["bug"] })), "mcp__github__list: labels: [\"bug\"]");
        assert_eq!(called("advisor", &json!({})), "advisor", "it takes no input");
    }

    #[test]
    fn a_subagents_end_says_which_it_was() {
        let mut printer = Printer::default();
        printer.subagent(1, Some("Find the tests"), &"toolu_a".into());
        printer.subagent(2, None, &"toolu_b".into());

        assert_eq!(printer.ending(&"toolu_a".into(), SubagentOutcome::Finished), "a subagent finished: Find the tests");
        assert_eq!(printer.ending(&"toolu_b".into(), SubagentOutcome::Stopped), "a subagent stopped: toolu_b");
        assert_eq!(
            printer.ending(&"toolu_c".into(), SubagentOutcome::Failed),
            "a subagent failed: toolu_c",
            "one it didn't see start"
        );
    }
}
