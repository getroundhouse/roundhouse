//! What the screens' chats say. It's all made up, about a change this repo might make, so it reads like real work.

#![cfg(test)]

use protocol::event::{AgentRunState, AgentTask, ContentBlock, EventBody, TurnOutcome};
use protocol::model::{ApprovalId, MessageId, TaskId, ToolUseId};
use protocol::tail::{BlockKind, Tail, TailBlock, TailMessage};
use serde_json::json;

const ASK: &str = "The sidebar should show each workspace's PR state. Where would that come from?";

/// A turn that's over: a prompt, two tools and a reply with a list and some code.
pub(crate) fn finished() -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(text(
        "msg-3",
        "The sidebar only knows what's in `Workspace`, so the PR state has to reach it through the feed.\n\n\
         **Two ways to get it there:**\n\n\
         1. The hub asks GitHub for each workspace's branch, and puts the answer on `Workspace`. Every client gets \
         it from the feed, and GitHub is asked once.\n\
         2. Each client asks GitHub itself, the way the header's PR button does now.\n\n\
         I'd go with the first. The field would look like this:\n\n\
         ```rust\npub struct Workspace {\n    // ...\n    /// As the hub last saw it. `None` if there isn't one.\n    \
         pub pull_request: Option<PullRequest>,\n}\n```",
    ));
    events.push(EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: Some(48_000),
        cost_usd: Some(0.21),
        num_turns: Some(3),
    });
    events
}

/// A turn that's over with the test run it started still going in the background, which the agent comes back to.
pub(crate) fn background() -> Vec<EventBody> {
    let mut events = looked_around();
    let command = json!({ "command": "cargo test --workspace", "description": "Run the full test suite",
        "run_in_background": true });
    events.push(tool_use("msg-3", "toolu-3", "Bash", command));
    let suite = AgentTask { id: TaskId::from("b1"), description: Some("Run the full test suite".to_string()) };
    events.push(EventBody::AgentTasks { tasks: vec![suite.clone()], stopped: Vec::new() });
    events.push(tool_result("toolu-3", "Command running in background with ID: b1"));
    let watch = json!({ "command": "tail -f target/test.log | grep --line-buffered FAILED",
        "description": "Watch the run for failures", "run_in_background": true });
    events.push(tool_use("msg-4", "toolu-4", "Bash", watch));
    let watcher = AgentTask { id: TaskId::from("b2"), description: Some("Watch the run for failures".to_string()) };
    events.push(EventBody::AgentTasks { tasks: vec![suite, watcher], stopped: Vec::new() });
    events.push(tool_result("toolu-4", "Command running in background with ID: b2"));
    events.push(text(
        "msg-5",
        "The full test suite is running in the background, since it takes a few minutes. I'll pick up again when it \
         finishes, or sooner if something fails.",
    ));
    events.push(EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: Some(31_000),
        cost_usd: Some(0.14),
        num_turns: Some(4),
    });
    events
}

/// A short turn that's over: a prompt and a reply, and nothing else.
pub(crate) fn answered(prompt: &str, reply: &str) -> Vec<EventBody> {
    vec![
        EventBody::UserPrompt { text: prompt.to_string(), client_msg_id: None, attachments: Vec::new() },
        text("msg-1", reply),
        EventBody::TurnEnded {
            outcome: TurnOutcome::Success,
            result: None,
            errors: Vec::new(),
            duration_ms: Some(9_000),
            cost_usd: Some(0.04),
            num_turns: Some(1),
        },
    ]
}

/// Partway through a turn, with a reply still being written. The rest of it is [`streaming_tail`].
pub(crate) fn streaming() -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(EventBody::AgentState { state: AgentRunState::Running });
    events
}

pub(crate) fn streaming_tail() -> Tail {
    Tail {
        messages: vec![TailMessage {
            run: None,
            message_id: MessageId::from("msg-3"),
            parent_tool_use_id: None,
            blocks: vec![TailBlock {
                index: 0,
                kind: BlockKind::Text,
                tool_name: None,
                text:
                    "The feed already sends the whole `Workspace` record whenever it changes, so a new field reaches \
                       every client without anything else. What it doesn't have yet is a way to"
                        .to_string(),
            }],
        }],
    }
}

/// Waiting for someone to allow a command.
pub(crate) fn approval() -> Vec<EventBody> {
    let mut events = looked_around();
    events
        .push(text("msg-3", "I'll run the desktop app's UI tests before changing anything, to see where they stand."));
    events.push(tool_use("msg-4", "toolu-3", "Bash", json!({ "command": "cargo test -p desktop --test ui" })));
    events.push(EventBody::ApprovalRequested {
        approval_id: ApprovalId::from("approval-1"),
        tool_name: "Bash".to_string(),
        tool_use_id: Some(ToolUseId::from("toolu-3")),
        input: json!({ "command": "cargo test -p desktop --test ui", "description": "Run the desktop UI tests" }),
        description: None,
        suggestions: serde_json::Value::Null,
    });
    events
}

/// Waiting for an answer to a question with choices.
pub(crate) fn question() -> Vec<EventBody> {
    let mut events = looked_around();
    let input = json!({ "questions": [{
        "question": "Where should the PR state be fetched?",
        "header": "PR state",
        "multiSelect": false,
        "options": [
            { "label": "In the hub", "description": "Asked once, and every client gets it from the feed" },
            { "label": "In each client", "description": "Simpler, but every window asks GitHub on its own" },
        ],
    }]});
    events.push(tool_use("msg-3", "toolu-3", "AskUserQuestion", input.clone()));
    events.push(EventBody::ApprovalRequested {
        approval_id: ApprovalId::from("approval-1"),
        tool_name: "AskUserQuestion".to_string(),
        tool_use_id: Some(ToolUseId::from("toolu-3")),
        input,
        description: None,
        suggestions: serde_json::Value::Null,
    });
    events
}

/// The start every chat here shares: the prompt, a search and a file read.
fn looked_around() -> Vec<EventBody> {
    let mut events = vec![
        EventBody::UserPrompt { text: ASK.to_string(), client_msg_id: None, attachments: Vec::new() },
        EventBody::AgentInit {
            agent_session_id: None,
            model: Some("claude-opus".to_string()),
            cwd: Some("/workspaces/aspen".to_string()),
            permission_mode: Some("acceptEdits".to_string()),
            agent_version: None,
        },
        text("msg-1", "I'll start with how the sidebar gets its records."),
    ];
    events.push(tool_use(
        "msg-1",
        "toolu-1",
        "Bash",
        json!({ "command": "rg -n 'HubNotice::Workspace' crates apps", "description": "Find where workspace notices go" }),
    ));
    events.push(tool_result(
        "toolu-1",
        "crates/hub/src/hub/workspaces.rs:224:        .inspect(|workspace| self.notify(HubNotice::Workspace(workspace.clone())))\n\
         apps/desktop/src/app.rs:212:            HubNotice::Workspace(workspace) => {",
    ));
    events.push(tool_use(
        "msg-2",
        "toolu-2",
        "Read",
        json!({ "file_path": "/workspaces/aspen/apps/desktop/src/sidebar.rs" }),
    ));
    events.push(tool_result("toolu-2", "     1\t//! Workspaces grouped by repository, each with its status.\n     2\t\n     3\tuse gpui_kit::component::button::{Button, ButtonVariants as _};"));
    events
}

fn text(message: &str, text: &str) -> EventBody {
    EventBody::Assistant {
        message_id: Some(MessageId::from(message)),
        parent_tool_use_id: None,
        model: Some("claude-opus".to_string()),
        blocks: vec![ContentBlock::Text { text: text.to_string() }],
        first_index: None,
    }
}

fn tool_use(message: &str, id: &str, name: &str, input: serde_json::Value) -> EventBody {
    EventBody::Assistant {
        message_id: Some(MessageId::from(message)),
        parent_tool_use_id: None,
        model: Some("claude-opus".to_string()),
        blocks: vec![ContentBlock::ToolUse { id: id.to_string(), name: name.to_string(), input }],
        first_index: None,
    }
}

fn tool_result(id: &str, text: &str) -> EventBody {
    EventBody::AgentUser {
        parent_tool_use_id: None,
        blocks: vec![ContentBlock::ToolResult {
            tool_use_id: ToolUseId::from(id),
            is_error: false,
            text: text.to_string(),
            image_count: 0,
        }],
    }
}

pub(crate) const SETUP_LOG: &str = "\
$ cp ../.env.example .env
$ pnpm install --frozen-lockfile
Lockfile is up to date, resolution step is skipped
Progress: resolved 1, reused 0, downloaded 0, added 0
 ERR_PNPM_FETCH_404  GET https://registry.example.com/@signal-box%2Fschedule: Not Found - 404

This error happened while installing a direct dependency of /workspaces/aspen

@signal-box/schedule is not in the npm registry, or you have no permission to fetch it.
setup exited with code 1";
