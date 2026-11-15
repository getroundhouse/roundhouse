//! What a turn writes: its messages, streamed and finished, its calls and what comes back for them, each naming the
//! subagent it's from, if it's one's.

use crate::{Config, Input, Out, Prompt, cache, emit, pause};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// What a streamed block holds.
#[derive(Clone, Copy)]
pub(crate) enum Block {
    Text,
    Thinking,
}

impl Block {
    /// Its type, which is also the field its text goes in.
    pub(super) fn name(self) -> &'static str {
        match self {
            Block::Text => "text",
            Block::Thinking => "thinking",
        }
    }
}

/// How many messages this process has started.
static MESSAGES: AtomicU32 = AtomicU32::new(0);

/// A new message's ID. It's unique within a session, as the real CLI's are, so it counts across turns, and a resumed
/// conversation is another process.
pub(crate) fn message_id() -> String {
    let number = MESSAGES.fetch_add(1, Ordering::Relaxed) + 1;
    format!("msg_fake_{}_{number}", std::process::id())
}

pub(crate) struct Turn<'a> {
    pub(crate) config: &'a Config,
    pub(crate) out: &'a Out,
    /// The subagent whose messages these are, if they're a subagent's.
    pub(crate) within: Option<Within<'a>>,
}

/// The subagent whose messages a turn's are, as its lines name it.
#[derive(Clone, Copy)]
pub(crate) struct Within<'a> {
    /// The call that started it, which every line of its names as its parent.
    pub(crate) call: &'a str,
    /// The task it runs as, which its messages and what comes back for its calls name as `agent_id`, as 2.1.322's
    /// do. Whether the real CLI's streamed events name it wasn't seen, so these don't, and the adapter finds the
    /// subagent by the call.
    pub(crate) task: &'a str,
}

impl Turn<'_> {
    /// The call that started the subagent whose messages these are, if they're a subagent's.
    pub(crate) fn parent(&self) -> Option<&str> {
        self.within.map(|within| within.call)
    }

    /// `line`, naming the task its subagent runs as, if it's a subagent's.
    pub(crate) fn named(&self, mut line: Value) -> Value {
        if let Some(within) = self.within {
            line["agent_id"] = within.task.into();
        }
        line
    }

    pub(crate) fn stream_event(&self, message_id: &str, event: Value) {
        emit(
            self.out,
            json!({"type":"stream_event","event":event,"session_id":self.config.session_id,
            "parent_tool_use_id":self.parent(),"api_message_id":message_id}),
        );
    }

    pub(crate) fn finish_message(&self, message_id: &str, block: Value) {
        emit(
            self.out,
            self.named(json!({"type":"assistant","message":{"id":message_id,"model":"fake-model","role":"assistant",
            "content":[block]},"parent_tool_use_id":self.parent(),"session_id":self.config.session_id})),
        );
    }

    /// Returns true if interrupted part way.
    pub(crate) fn say(
        &mut self,
        block: Block,
        chunks: impl IntoIterator<Item = String>,
        gap: Duration,
        rx: &Receiver<Input>,
        queued: &mut VecDeque<Prompt>,
        eof: &mut bool,
    ) -> bool {
        let id = message_id();
        cache::message_start(self, &id);
        let name = block.name();
        self.stream_event(&id, json!({"type":"content_block_start","index":0,"content_block":{"type":name,(name):""}}));
        let mut text = String::new();
        for chunk in chunks {
            let delta = json!({"type":format!("{name}_delta"),(name):chunk});
            self.stream_event(&id, json!({"type":"content_block_delta","index":0,"delta":delta}));
            text.push_str(&chunk);
            if gap > Duration::ZERO && pause(rx, gap, queued, eof) {
                return true;
            }
        }
        // The real CLI sends the finished block before content_block_stop.
        self.finish_message(&id, json!({"type":name,(name):text}));
        self.stream_event(&id, json!({"type":"content_block_stop","index":0}));
        self.stream_event(&id, json!({"type":"message_stop"}));
        false
    }

    /// A call to the tool `name`, rather than a command.
    pub(crate) fn named_tool_use(&mut self, name: &str, input: Value) -> String {
        let id = message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":name,"input":input}));
        tool_use_id
    }

    pub(crate) fn tool_use(&mut self, command: &str) -> String {
        let id = message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":"Bash","input":{"command":command}}));
        tool_use_id
    }

    pub(crate) fn tool_result(&self, tool_use_id: &str, content: &str, is_error: bool) {
        emit(self.out, self.result_line(tool_use_id, content, is_error));
    }

    /// The result of a call the person denied, which tells the agent `message`, with the mark the real CLI puts beside
    /// it, as `claude_full_access.jsonl` has it.
    pub(crate) fn denied_result(&self, tool_use_id: &str, message: &str) {
        let mut line = self.result_line(tool_use_id, message, true);
        let rejected = json!({"decision":"reject","source":"user_reject"});
        line["tool_result_meta"] = json!([{"id":tool_use_id,"permission_decision":rejected}]);
        emit(self.out, line);
    }

    /// The line a tool's result comes in.
    pub(crate) fn result_line(&self, tool_use_id: &str, content: &str, is_error: bool) -> serde_json::Value {
        self.named(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result",
        "tool_use_id":tool_use_id,"content":content,"is_error":is_error}]},"parent_tool_use_id":self.parent(),
        "session_id":self.config.session_id}))
    }
}
