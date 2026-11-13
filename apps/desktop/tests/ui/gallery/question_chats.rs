//! What the agent asks in the screens' chats, about where the sidebar's PR state should come from: waiting to be
//! answered, and once it's answered, when the transcript keeps it as a call's line, whose questions and answers are
//! copied with the rest of the chat while it's open.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use protocol::event::{ApprovalOutcome, EventBody};
use protocol::fixtures::{ApprovalRequest, approval_request};
use protocol::model::{SessionStatus, ToolUseId};
use serde_json::{Value, json};

use super::capture;
use super::chats::looked_around;
use super::events::{replied, text, tool_result, tool_use};
use super::worlds::chat_world;

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
    let mut events = question();
    events.extend(answer("toolu-3", "approval-1", &[(PR_STATE, "In the hub")]));
    events.push(text("msg-4", TWO_MORE));
    events.extend(asking("msg-5", "toolu-4", "approval-2", json!({ "questions": [mark(), refresh()] })));
    events.extend(answer("toolu-4", "approval-2", &[(MARK, MARKED)]));
    events.push(text("msg-6", REPLY));
    events.push(replied(REPLY, 64, 0.18));
    events
}

/// What the agent says before it asks [`answered`]'s two more questions.
const TWO_MORE: &str = "The hub, then. Two more things decide how the sidebar shows it.";
/// What was answered to the first of them.
const MARKED: &str = "Open, Merged, Only while it's open";
/// What the agent replies once they're answered.
const REPLY: &str = "I'll have the hub ask GitHub for each workspace's pull request and keep it on the workspace's \
                     record. The mark will show it while it's open or merged, and only while the pull request is \
                     open. I've left how often the hub asks again as it is, since you didn't say.";

const PR_STATE: &str = "Where should the PR state be fetched?";
const MARK: &str = "Which states should the sidebar's mark show?";
const REFRESH: &str = "How often should the hub ask GitHub again?";

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
        "question": REFRESH,
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
        by_setting: false,
    };
    vec![closed, tool_result(call, &told)]
}

/// Each question an opened call shows, and what was answered to it, is copied with the rest, in the order shown, each
/// a blank line apart. Their headers, and what says nobody answered, aren't the agent's words or the person's, so a
/// copy leaves them out.
#[test]
fn an_opened_call_s_questions_and_answers_are_copied_with_the_rest() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let world = || chat_world(SessionStatus::Idle, answered());

    let copied = capture::copied_whole(world, &[TWO_ASKED], &text_system);

    let shown = [TWO_MORE, MARK, MARKED, REFRESH, REPLY].join("\n\n");
    assert!(copied.contains(&shown), "{copied:?}");
    assert_eq!(copied.matches(PR_STATE).count(), 0, "and nothing of the call that isn't open: {copied:?}");
}
