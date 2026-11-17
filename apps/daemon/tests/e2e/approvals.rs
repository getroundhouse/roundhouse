//! Approvals.

#![cfg(test)]

use std::path::PathBuf;

use client_core::calls::typed::output as said;
use client_core::{Client, ClientError};
use protocol::capability::approvals::{Asks, Decision};
use protocol::capability::calls::{CallKind, CallOutput};
use protocol::client::ErrorCode;
use protocol::event::{ContentBlock, Denied, EventBody, SessionEvent};
use protocol::fixtures::{allow_once, deny};
use protocol::model::{ApprovalId, SessionId, SessionStatus};

use crate::support::{Stack, read_all, wait_for_approval, wait_for_status, wait_for_turn_end};

/// A chat in plan mode, which asks before each command, and the worktree its commands run in.
struct Planning {
    stack: Stack,
    client: Client,
    chat: SessionId,
    worktree: PathBuf,
}

impl Planning {
    fn start() -> Self {
        let stack = Stack::start();
        let client = stack.client();
        let (workspace, chat) = stack.workspace_planning(&client);
        let worktree = PathBuf::from(workspace.path.unwrap());
        Self { stack, client, chat: chat.id, worktree }
    }

    /// Has the agent run `command` in the chat's `turn`th turn, decides the approval it asks for with `decision`,
    /// through `client`, and returns the approval and what that turn recorded.
    fn run(&self, client: &Client, turn: usize, command: &str, decision: Decision) -> (ApprovalId, Vec<SessionEvent>) {
        let before = read_all(client, &self.chat).len();
        client.session_prompt(&self.chat, &format!("tool:{command}"), None).unwrap();
        let approval = wait_for_approval(client, &self.chat);
        client.session_decide(&self.chat, &approval, decision).unwrap();
        (approval, wait_for_turn_end(client, &self.chat, turn)[before..].to_vec())
    }
}

/// Each way an approval is answered, one after another in one chat in plan mode, each the answer to a request of its
/// own, and what each leaves on the record.
#[test]
fn an_approval_is_answered_and_recorded_as_it_was_given_however_it_was_given() {
    let chat = Planning::start();

    allowed_by_a_different_client_once(&chat);
    denied_with_why_and_without(&chat);
    a_failed_command_recorded_as_one(&chat);
}

/// It doesn't run before it's approved, and a client other than the one that saw it asked, which has gone since, can
/// answer it, once: answering twice is an error rather than sending the agent a second response.
fn allowed_by_a_different_client_once(chat: &Planning) {
    let Planning { stack, chat: id, worktree, .. } = chat;
    let first = stack.client();
    first.session_prompt(id, "tool:echo approved > approved.txt", None).unwrap();
    let approval = wait_for_approval(&first, id);
    wait_for_status(&first, id, SessionStatus::WaitingApproval);
    assert!(!worktree.join("approved.txt").exists(), "must not run before it's approved");
    drop(first);

    let other = stack.client();
    other.session_decide(id, &approval, allow_once()).unwrap();
    let events = wait_for_turn_end(&other, id, 1);
    let twice = other.session_decide(id, &approval, allow_once());

    assert!(worktree.join("approved.txt").exists(), "it ran once the other client allowed it");
    let allowed = |body: &EventBody| {
        let EventBody::ApprovalResolved { approval_id, decision: Some(decision), .. } = body else { return false };
        (approval_id, &**decision) == (&approval, &allow_once())
    };
    assert!(events.iter().any(|e| allowed(&e.body)), "recorded as allowed");
    assert!(matches!(&twice, Err(ClientError::Api(e)) if e.code == ErrorCode::Conflict), "answered twice: {twice:?}");
}

/// Each result to a call, by what it told the agent, with what it says of a denial.
fn denials(events: &[SessionEvent]) -> Vec<(String, Option<Denied>)> {
    let blocks = events.iter().flat_map(|e| match &e.body {
        EventBody::AgentUser { blocks, .. } => blocks.as_slice(),
        _ => &[],
    });
    let result = |block: &ContentBlock| match block {
        ContentBlock::CallResult { output, is_error: true, denied, .. } => Some((said(output), denied.clone())),
        _ => None,
    };
    blocks.filter_map(result).collect()
}

/// A denied tool doesn't run, and the agent hears why, and its result says it was denied, and who said why. With
/// nothing said, the agent is told in the adapter's words, which aren't the person's.
fn denied_with_why_and_without(chat: &Planning) {
    let (_, events) = chat.run(&chat.client, 2, "echo nope > nope.txt", deny(Some("not that")));
    assert!(!chat.worktree.join("nope.txt").exists(), "a denied tool ran");
    let said = Denied { said: Some("not that".into()) };
    assert_eq!(denials(&events), [("not that".to_string(), Some(said))], "the agent is told, and it says who said");

    let (_, events) = chat.run(&chat.client, 3, "echo nope > nope.txt", deny(None));
    let (_, denied) = denials(&events).pop().expect("the denied call's result");
    assert_eq!(denied, Some(Denied { said: None }), "denied with nothing said");
}

/// So a client draws each from what's recorded, with none of the agent's names for its tools: the request to run a
/// command, the call, and what came back all say it's a command, and what came back says how the command exited
/// apart from what it printed.
fn a_failed_command_recorded_as_one(chat: &Planning) {
    let (_, events) = chat.run(&chat.client, 4, "echo kinds && exit 3", allow_once());

    let command = Some(CallKind::Command);
    let asked: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::ApprovalRequested { request, .. } => match &request.asks {
                Asks::Call { call } => Some(Some(call.kind())),
                _ => Some(None),
            },
            _ => None,
        })
        .collect();
    assert_eq!(asked, std::slice::from_ref(&command), "the request says it's a command");
    let blocks: Vec<_> = events
        .iter()
        .flat_map(|event| match &event.body {
            EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => blocks.as_slice(),
            _ => &[],
        })
        .filter(|block| matches!(block, ContentBlock::Call { .. } | ContentBlock::CallResult { .. }))
        .collect();
    let [
        ContentBlock::Call { call: made, .. },
        ContentBlock::CallResult { output: CallOutput::Command(printed), is_error, .. },
    ] = blocks.as_slice()
    else {
        panic!("the call, then what it printed: {blocks:?}")
    };
    assert_eq!(Some(made.kind()), command, "the call says it's a command");
    let ended = (*is_error, printed.exit_code, printed.output.as_str());
    assert_eq!(ended, (true, Some(3), "kinds\n"), "how it exited isn't in what it printed");
}
