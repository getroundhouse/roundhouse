//! Plain text for a terminal. This is a debugging view, not the product's transcript.

use std::collections::HashSet;
use std::io::Write;

use protocol::event::{ContentBlock, EventBody, SessionEvent, TurnOutcome};
use protocol::model::{MessageId, Seq};
use protocol::tail::{DeltaChange, StreamDelta, Tail};
use serde_json::Value;

use crate::run::short;

/// How much of a tool's result to show. The full text stays in the log.
const RESULT_PREVIEW_LINES: usize = 6;

#[derive(Default)]
pub(crate) struct Printer {
    /// Messages whose text has already been printed from the streaming tail.
    streamed: HashSet<MessageId>,
    mid_line: bool,
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

    pub(crate) fn event(&mut self, event: &SessionEvent) {
        match &event.body {
            EventBody::UserPrompt { text, .. } => self.line(event.seq, &format!("you: {text}")),
            EventBody::AgentStarted { pid, resumed } => {
                let how = if *resumed { "resumed" } else { "started" };
                self.line(event.seq, &format!("agent {how}, pid {pid}"));
            }
            EventBody::AgentInit { model, permission_mode, cwd, .. } => self.line(
                event.seq,
                &format!(
                    "agent ready: model {}, permissions {}, in {}",
                    model.as_deref().unwrap_or("?"),
                    permission_mode.as_deref().unwrap_or("?"),
                    cwd.as_deref().unwrap_or("?"),
                ),
            ),
            EventBody::Assistant { message_id, blocks, .. } => {
                let already_streamed = message_id.as_ref().is_some_and(|id| self.streamed.remove(id));
                for block in blocks {
                    if already_streamed && matches!(block, ContentBlock::Text { .. }) {
                        self.end_line();
                        continue;
                    }
                    self.block(event.seq, block);
                }
            }
            EventBody::AgentUser { blocks, .. } => {
                for block in blocks {
                    self.block(event.seq, block);
                }
            }
            EventBody::TurnEnded { outcome, duration_ms, cost_usd, errors, .. } => {
                self.turn_ended(event.seq, *outcome, duration_ms.unwrap_or(0), cost_usd.unwrap_or(0.0), errors);
            }
            EventBody::ApprovalRequested { approval_id, tool_name, input, description, .. } => {
                let summary = description.clone().unwrap_or_else(|| summarize_input(input));
                self.line(event.seq, &format!("APPROVAL NEEDED: {tool_name}: {summary}"));
                self.line(event.seq, &format!("  roundhouse approve {} {approval_id}", event.session_id));
            }
            EventBody::ApprovalResolved { approval_id, outcome } => {
                self.line(event.seq, &format!("approval {} {outcome:?}", short(approval_id.as_str())));
            }
            EventBody::AgentExited { reason, code, signal, stderr_tail } => {
                self.line(event.seq, &format!("agent exited: {reason:?}, code {code:?}, signal {signal:?}"));
                for line in stderr_tail {
                    self.line(event.seq, &format!("  {line}"));
                }
            }
            EventBody::Notice { level, message } => self.line(event.seq, &format!("{level:?}: {message}")),
            EventBody::AgentMode { permission_mode } => {
                self.line(event.seq, &format!("approval mode: {permission_mode}"));
            }
            EventBody::AgentState { .. } | EventBody::AgentOther { .. } | EventBody::Unknown => {}
        }
    }

    fn block(&mut self, seq: u64, block: &ContentBlock) {
        match block {
            ContentBlock::Text { text } => self.line(seq, text),
            ContentBlock::Thinking { text } if !text.is_empty() => {
                self.line(seq, &format!("(thinking) {}", first_line(text)));
            }
            ContentBlock::ToolUse { name, input, .. } => {
                self.line(seq, &format!("tool {name}: {}", summarize_input(input)));
            }
            ContentBlock::ToolResult { is_error, text, .. } => {
                let label = if *is_error { "tool error" } else { "tool result" };
                let lines: Vec<&str> = text.lines().collect();
                self.line(seq, &format!("{label}: {}", lines.first().copied().unwrap_or("")));
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

    /// Text is printed as it arrives, and everything else waits for the finished block.
    pub(crate) fn delta(&mut self, delta: &StreamDelta) {
        if let DeltaChange::Text { text, .. } = &delta.change {
            self.streamed.insert(delta.message_id.clone());
            self.mid_line = true;
            print!("{text}");
            std::io::stdout().flush().ok();
        }
    }

    pub(crate) fn tail(&mut self, tail: &Tail) {
        for message in &tail.messages {
            for block in &message.blocks {
                if block.kind == protocol::tail::BlockKind::Text && !block.text.is_empty() {
                    self.streamed.insert(message.message_id.clone());
                    self.mid_line = true;
                    print!("{}", block.text);
                }
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

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn summarize_input(input: &Value) -> String {
    for key in ["command", "file_path", "path", "pattern", "url", "query", "description"] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            return first_line(value).to_string();
        }
    }
    let text = input.to_string();
    text.chars().take(120).collect()
}
