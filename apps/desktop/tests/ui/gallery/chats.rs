//! What the screens' chats say. It's all made up, mostly about a change this repo might make, so it reads like real
//! work.

#![cfg(test)]

use protocol::attachment::Attachment;
use protocol::event::{AgentRunState, AgentTask, EventBody, TurnOutcome};
use protocol::model::{ApprovalId, MessageId, TaskId, ToolUseId};
use protocol::tail::{BlockKind, Tail, TailBlock, TailMessage};
use serde_json::json;

use super::events::{replied, taken, text, thought, tool_result, tool_use, user_prompt};

const ASK: &str = "The sidebar should show each workspace's PR state. Where would that come from?";

/// The reply that ends [`finished`].
const REPLY: &str = "The sidebar only knows what's in `Workspace`, so the PR state has to reach it through the feed.\n\n\
     **Two ways to get it there:**\n\n\
     1. The hub asks GitHub for each workspace's branch, and puts the answer on `Workspace`. Every client gets it from \
     the feed, and GitHub is asked once.\n\
     2. Each client asks GitHub itself, the way the header's PR button does now.\n\n\
     I'd go with the first. The field would look like this:\n\n\
     ```rust\npub struct Workspace {\n    // ...\n    /// As the hub last saw it. `None` if there isn't one.\n    \
     pub pull_request: Option<PullRequest>,\n}\n```";

/// A turn that's over: a prompt, two tools and a reply with a list and some code.
pub(crate) fn finished() -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(text("msg-3", REPLY));
    events.push(replied(REPLY, 106, 0.21));
    events
}

/// A turn that's over with the test run it started still going in the background, which the agent comes back to.
pub(crate) fn background() -> Vec<EventBody> {
    let reply = "The full test suite is running in the background, since it takes a few minutes. I'll pick up again \
                 when it finishes, or sooner if something fails.";
    let mut events = started_in_background();
    events.extend([text("msg-5", reply), replied(reply, 31, 0.14)]);
    events
}

/// Partway through a turn, with the test run it started, and a watch on it, going in the background.
pub(crate) fn started_in_background() -> Vec<EventBody> {
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
    events
}

/// A short turn that's over: a prompt and a reply, and nothing else.
pub(crate) fn answered(prompt: &str, reply: &str) -> Vec<EventBody> {
    vec![user_prompt(prompt), taken(1), text("msg-1", reply), replied(reply, 9, 0.04)]
}

/// The reply that ends [`attached`].
const ATTACHED_REPLY: &str = "The badge comes after the name in the row, but the name isn't allowed to shrink, so at \
     this width the badge is pushed over it rather than the name being cut short. Letting the name shrink and end in \
     an ellipsis, with the badge kept at its own width, fixes it.";

/// A prompt carrying a screenshot and a log, and the reply to it.
pub(crate) fn attached(attachments: Vec<Attachment>) -> Vec<EventBody> {
    vec![
        EventBody::UserPrompt {
            text: "When the sidebar is narrow, the PR badge runs over the workspace's name. This is how it looks, \
                   with the layout log."
                .to_string(),
            client_msg_id: None,
            attachments,
        },
        taken(1),
        text("msg-1", ATTACHED_REPLY),
        replied(ATTACHED_REPLY, 21, 0.08),
    ]
}

pub(crate) const LAYOUT_LOG: &str = "\
row aspen: width 212, name 148 (min 148), badge 58
row aspen: overflow 6 px, badge moved left over the name
";

/// Partway through a turn, with a reply still being written. The rest of it is [`streaming_tail`].
pub(crate) fn streaming() -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(EventBody::AgentState { state: AgentRunState::Running });
    events
}

pub(crate) fn streaming_tail() -> Tail {
    writing(
        "msg-3",
        BlockKind::Text,
        "The feed already sends the whole `Workspace` record whenever it changes, so a new field reaches every client \
         without anything else. What it doesn't have yet is a way to",
    )
}

/// The next call [`started_in_background`] is writing, which doesn't show until it's recorded.
pub(crate) fn calling_tail() -> Tail {
    let mut tail = writing("msg-5", BlockKind::ToolUse, r#"{"command": "cargo test -p desk"#);
    tail.messages[0].blocks[0].tool_name = Some("Bash".to_string());
    tail
}

/// A message the agent is still writing, with one block of `kind` so far.
fn writing(message: &str, kind: BlockKind, text: &str) -> Tail {
    Tail {
        messages: vec![TailMessage {
            run: None,
            message_id: MessageId::from(message),
            parent_tool_use_id: None,
            blocks: vec![TailBlock { index: 0, kind, tool_name: None, text: text.to_string() }],
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

/// Running the UI tests, with a prompt sent meanwhile that the agent takes once they finish.
pub(crate) fn steered() -> Vec<EventBody> {
    let mut events = looked_around();
    events
        .push(text("msg-3", "I'll run the desktop app's UI tests before changing anything, to see where they stand."));
    events.push(tool_use("msg-4", "toolu-3", "Bash", json!({ "command": "cargo test -p desktop --test ui" })));
    events.push(EventBody::UserPrompt {
        text: "Only the sidebar's tests matter here, so skip the rest next time.".to_string(),
        client_msg_id: None,
        attachments: Vec::new(),
    });
    events
}

/// Stopped partway, after looking around.
pub(crate) fn stopped() -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(EventBody::TurnEnded {
        outcome: TurnOutcome::Cancelled,
        result: None,
        errors: Vec::new(),
        duration_ms: Some(21_000),
        cost_usd: Some(0.09),
        num_turns: Some(2),
        started_at_ms: None,
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

const FIRST_THOUGHT: &str = "The test says a train should slow at caution, not stop. `approach` brakes whenever the \
                             signal isn't clear, so it treats caution like stop.

That's the bug: it should brake only at stop, and slow at caution. I'll check how `brake` and `slow` differ before \
changing it, since `slow` might need a speed.";

/// Thinking between calls, about the signalling crate the tool calls' chats work on: a long thought opened, and a
/// short one closed.
pub(crate) fn thinking() -> Vec<EventBody> {
    let reply = "The train treats caution as stop: `approach` brakes for any signal that isn't clear. It should brake \
                 only at stop, and slow to half speed at caution.";
    let mut events = thought_then_read();
    events.extend([
        thought("msg-3", "`slow` takes a target speed, so caution can pass half the line speed."),
        text("msg-4", reply),
        replied(reply, 18, 0.07),
    ]);
    events
}

/// Partway through [`thinking`], with its second thought still being written. The rest of it is
/// [`thinking_tail`].
pub(crate) fn thinking_so_far() -> Vec<EventBody> {
    let mut events = thought_then_read();
    events.push(EventBody::AgentState { state: AgentRunState::Running });
    events
}

pub(crate) fn thinking_tail() -> Tail {
    writing("msg-3", BlockKind::Thinking, "So slow takes a target speed, and caution can pass half the line")
}

/// The prompt, a thought about it, and a look at the code it's about.
fn thought_then_read() -> Vec<EventBody> {
    vec![
        user_prompt("Why does the train stop at a caution signal in the depot test?"),
        taken(1),
        thought("msg-1", FIRST_THOUGHT),
        tool_use("msg-2", "toolu-1", "Read", json!({ "file_path": "/workspaces/aspen/crates/signals/src/train.rs" })),
        tool_result(
            "toolu-1",
            "    52\t    fn approach(&mut self, signal: Aspect) {\n    53\t        if signal != Aspect::Clear {\n    54\t            self.brake();",
        ),
    ]
}

/// The start every chat here shares: the prompt, a search and a file read.
fn looked_around() -> Vec<EventBody> {
    let mut events = vec![
        user_prompt(ASK),
        taken(1),
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

pub(crate) const SETUP_LOG: &str = "\
$ cp ../.env.example .env
$ pnpm install --frozen-lockfile
Lockfile is up to date, resolution step is skipped
Progress: resolved 1, reused 0, downloaded 0, added 0
 ERR_PNPM_FETCH_404  GET https://registry.example.com/@signal-box%2Fschedule: Not Found - 404

This error happened while installing a direct dependency of /workspaces/aspen

@signal-box/schedule is not in the npm registry, or you have no permission to fetch it.
setup exited with code 1";
