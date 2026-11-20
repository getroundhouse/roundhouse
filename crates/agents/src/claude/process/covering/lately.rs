//! What the agent has been doing lately, as its own events say, which the stand-in is told so it says only what the
//! agent is doing.

use std::collections::VecDeque;

use protocol::event::{ContentBlock, EventBody};

/// How many of the agent's latest doings the stand-in is told, oldest first.
const KEPT: usize = 16;

/// What the agent has been doing, and whether it's on a turn.
#[derive(Default)]
pub(super) struct Lately {
    doings: VecDeque<String>,
    /// Whether it's on a turn, from when it takes a prompt or starts one by itself until the turn ends, so what's said
    /// meanwhile finds it busy with something else, rather than thinking about it.
    pub(super) in_turn: bool,
    /// How many of its tasks run in the background, as it last said.
    in_background: usize,
}

impl Lately {
    /// Takes in one of the agent's events. A subagent's own are left out, since the stand-in speaks for the agent, and
    /// a subagent's start and end say what the agent needs to of it.
    pub(super) fn heard(&mut self, body: &EventBody) {
        match body {
            EventBody::PromptTaken { .. } => self.in_turn = true,
            EventBody::OwnTurn(_) => {
                self.in_turn = true;
                self.note("started a turn by itself, after something it was waiting on ended".into());
            }
            EventBody::TurnEnded { .. } => {
                self.in_turn = false;
                self.note("finished its turn".into());
            }
            EventBody::AgentTasks { tasks, .. } => self.in_background = tasks.len(),
            EventBody::SubagentStarted { description, in_background, .. } => {
                let place = if *in_background { " in the background" } else { "" };
                let what = description.as_deref().unwrap_or("a task");
                self.note(format!("started a subagent{place}: {}", clip(what, 120)));
            }
            EventBody::SubagentEnded { report, .. } => {
                let found = report.as_deref().map_or_else(String::new, |report| format!(": {}", clip(report, 200)));
                self.note(format!("a subagent finished{found}"));
            }
            EventBody::Assistant { subagent: None, blocks, .. } => {
                for block in blocks {
                    match block {
                        ContentBlock::Thinking { text } => self.note(format!("thought: {}", clip(text, 200))),
                        ContentBlock::Text { text, .. } => self.note(format!("said: {}", clip(text, 160))),
                        ContentBlock::Call { call, .. } => self.note(called(call)),
                        ContentBlock::CallResult { .. } | ContentBlock::Image { .. } | ContentBlock::Unknown => {}
                    }
                }
            }
            EventBody::UserPrompt { .. }
            | EventBody::PromptDropped { .. }
            | EventBody::AgentStarted { .. }
            | EventBody::AgentInit { .. }
            | EventBody::AgentState { .. }
            | EventBody::TaskChanged { .. }
            | EventBody::AgentLimits { .. }
            | EventBody::SubagentContext { .. }
            | EventBody::Assistant { .. }
            | EventBody::AgentUser { .. }
            | EventBody::ToolCallDropped { .. }
            | EventBody::RequestFailed { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalResolved { .. }
            | EventBody::AgentExited { .. }
            | EventBody::Notice { .. }
            | EventBody::AgentMode { .. }
            | EventBody::AgentOther { .. }
            | EventBody::AgentCompacting { .. }
            | EventBody::CompactionAsked { .. }
            | EventBody::AgentCompacted { .. }
            | EventBody::CacheExpired { .. }
            | EventBody::ConversationStarted { .. }
            | EventBody::VoiceCut { .. }
            | EventBody::Unknown => {}
        }
    }

    /// The person said `text`, typed or out loud.
    pub(super) fn said_to(&mut self, text: &str) {
        self.note(format!("the person said: {}", clip(text, 160)));
    }

    /// What the stand-in is asked, of the person having just said `text`.
    pub(super) fn asking(&self, text: &str) -> String {
        let doing = if self.in_turn {
            "You're in the middle of something else."
        } else {
            "You're thinking about what they just said."
        };
        let in_background = match self.in_background {
            0 => String::new(),
            1 => " You have 1 thing running in the background.".into(),
            n => format!(" You have {n} things running in the background."),
        };
        let doings: Vec<String> = self.doings.iter().map(|doing| format!("- {doing}")).collect();
        format!(
            "The person just said: \"{text}\"\n\n{doing}{in_background} What you've been doing lately, oldest \
             first:\n{}",
            doings.join("\n")
        )
    }

    fn note(&mut self, doing: String) {
        self.doings.push_back(doing);
        while self.doings.len() > KEPT {
            self.doings.pop_front();
        }
    }
}

/// A call, in a line: its kind, and what it's on, as the first of the fields that say so in its serde form.
fn called(call: &protocol::capability::calls::Call) -> String {
    let form = serde_json::to_value(call).unwrap_or_default();
    let kind = form["kind"].as_str().unwrap_or("tool").replace('_', " ");
    let on = ["description", "command", "path", "pattern", "query", "url", "prompt", "name"]
        .iter()
        .find_map(|field| form[field].as_str())
        .map_or_else(String::new, |on| format!(": {}", clip(on, 120)));
    format!("called {kind}{on}")
}

/// `text` on one line, cut to `most` characters.
fn clip(text: &str, most: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= most { line } else { format!("{}…", line.chars().take(most).collect::<String>()) }
}
