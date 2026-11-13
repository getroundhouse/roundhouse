//! What the agent asks in the screens' chats, about where the sidebar's PR state should come from: waiting to be
//! answered, and once it's answered, when the transcript keeps it as a call's line.

#![cfg(test)]

use protocol::event::{ApprovalOutcome, EventBody};
use protocol::fixtures::{ApprovalRequest, approval_request};
use protocol::model::ToolUseId;
use serde_json::{Value, json};

use super::chats::looked_around;
use super::events::{replied, text, tool_result, tool_use};

/// The line [`answered`] leaves for its second call, with two questions.
pub(crate) const TWO_ASKED: &str = "Asked 2 questions";

/// Waiting for an answer to a question with choices.
pub(crate) fn question() -> Vec<EventBody> {
    let mut events = looked_around();
    events.extend(asking("msg-3", "toolu-3", "approval-1", json!({ "questions": [pr_state()] })));
    events
}

/// Waiting for answers to three questions, the second of which takes several.
pub(crate) fn questions() -> Vec<EventBody> {
    let mut events = looked_around();
    events.extend(asking("msg-3", "toolu-3", "approval-1", json!({ "questions": [pr_state(), mark(), refresh()] })));
    events
}

/// The question of [`question`] answered, then two more, of which only the first was answered, as their answers were
/// recorded, and the reply that followed.
pub(crate) fn answered() -> Vec<EventBody> {
    let reply = "I'll have the hub ask GitHub for each workspace's pull request and keep it on the workspace's record. \
                 The mark will show it while it's open or merged, and only while the pull request is open. I've left \
                 how often the hub asks again as it is, since you didn't say.";
    let mut events = question();
    events.extend(answer("toolu-3", "approval-1", &[(PR_STATE, "In the hub")]));
    events.push(text("msg-4", "The hub, then. Two more things decide how the sidebar shows it."));
    events.extend(asking("msg-5", "toolu-4", "approval-2", json!({ "questions": [mark(), refresh()] })));
    events.extend(answer("toolu-4", "approval-2", &[(MARK, "Open, Merged, Only while it's open")]));
    events.push(text("msg-6", reply));
    events.push(replied(reply, 64, 0.18));
    events
}

const PR_STATE: &str = "Where should the PR state be fetched?";
const MARK: &str = "Which states should the sidebar's mark show?";

fn pr_state() -> Value {
    json!({
        "question": PR_STATE,
        "header": "PR state",
        "multiSelect": false,
        "options": [
            { "label": "In the hub", "description": "Asked once, and every client gets it from the feed" },
            { "label": "In each client", "description": "Simpler, but every window asks GitHub on its own" },
        ],
    })
}

fn mark() -> Value {
    json!({
        "question": MARK,
        "header": "Mark",
        "multiSelect": true,
        "options": [
            { "label": "Open", "description": "Green, while it waits for review" },
            { "label": "Merged", "description": "Purple, until the workspace is archived" },
            { "label": "Closed", "description": "A muted red" },
            { "label": "Draft", "description": "A grey of its own" },
        ],
    })
}

fn refresh() -> Value {
    json!({
        "question": "How often should the hub ask GitHub again?",
        "header": "Refresh",
        "multiSelect": false,
        "options": [{ "label": "Every minute" }, { "label": "Every five minutes" }],
    })
}

/// The agent asking, with `input` as the question tool's input, in the message `message`.
fn asking(message: &str, call: &str, approval: &str, input: Value) -> Vec<EventBody> {
    vec![
        tool_use(message, call, "AskUserQuestion", input.clone()),
        ApprovalRequest {
            tool_use_id: Some(ToolUseId::from(call)),
            input,
            ..approval_request(approval, "AskUserQuestion")
        }
        .into(),
    ]
}

/// The answers to the questions the call `call` asked, as the hub records them, and as the agent is told them.
fn answer(call: &str, approval: &str, answers: &[(&str, &str)]) -> Vec<EventBody> {
    let told: Vec<String> = answers.iter().map(|(question, answer)| format!("\"{question}\"=\"{answer}\"")).collect();
    let told = format!(
        "Your questions have been answered: {}. You can now continue with these answers in mind.",
        told.join(", ")
    );
    let answers = answers.iter().map(|(question, answer)| ((*question).to_string(), (*answer).to_string())).collect();
    let closed = EventBody::ApprovalResolved {
        approval_id: approval.into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: Some(answers),
        answers_by_position: None,
    };
    vec![closed, tool_result(call, &told)]
}
