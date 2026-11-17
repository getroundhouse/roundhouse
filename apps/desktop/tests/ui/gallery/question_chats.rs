//! What the agent asks in the screens' chats, about where the sidebar's PR state should come from: waiting to be
//! answered, and once it's answered, when the transcript keeps it as a call's line, whose questions and answers are
//! copied with the rest of the chat while it's open.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use protocol::capability::approvals::{Asks, Decision};
use protocol::capability::asking::{Choose, Question, QuestionAnswer, QuestionOption, TypedAnswer};
use protocol::capability::calls::Call;
use protocol::event::EventBody;
use protocol::fixtures::{ApprovalRequest, approval_request, decided};
use protocol::model::{SessionStatus, ToolUseId};

use super::capture;
use super::chats::looked_around;
use super::worlds::chat_world;
use client_core::fake_hub::events::{replied, said as said_to, text, typed_call};

/// The line [`answered`] leaves for its second call, with two questions.
pub(crate) const TWO_ASKED: &str = "Asked 2 questions";

/// Waiting for an answer to a question with choices.
pub(crate) fn question() -> Vec<EventBody> {
    let mut events = looked_around();
    events.extend(asking("msg-3", "toolu-3", "approval-1", vec![pr_state()]));
    events
}

/// A question that asks only for words, without any offered choices.
pub(crate) fn written_answer() -> Vec<EventBody> {
    let mut events = looked_around();
    let words = asked("How should the sidebar behave while the hub reconnects?", "Reconnecting", false, &[]);
    events.extend(asking("msg-3", "toolu-3", "approval-1", vec![words]));
    events
}

/// Waiting for answers to three questions, the second of which takes several.
pub(crate) fn questions() -> Vec<EventBody> {
    let mut events = looked_around();
    events.extend(asking("msg-3", "toolu-3", "approval-1", vec![pr_state(), mark(), refresh()]));
    events
}

/// The question of [`question`] answered, then two more, of which only the first was answered, as their answers were
/// recorded, and the reply that followed.
pub(crate) fn answered() -> Vec<EventBody> {
    let mut events = question();
    events.extend(answer("toolu-3", "approval-1", &[pr_state()], vec![picked(&[0], None)]));
    events.push(text("msg-4", TWO_MORE));
    events.extend(asking("msg-5", "toolu-4", "approval-2", vec![mark(), refresh()]));
    let marked = picked(&[0, 1], Some("Only while it's open"));
    events.extend(answer("toolu-4", "approval-2", &[mark(), refresh()], vec![marked, QuestionAnswer::default()]));
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

fn pr_state() -> Question {
    let options = [
        ("In the hub", Some("Asked once, and every client gets it from the feed")),
        ("In each client", Some("Simpler, but every window asks GitHub on its own")),
    ];
    asked(PR_STATE, "PR state", false, &options)
}

fn mark() -> Question {
    let options = [
        ("Open", Some("Green, while it waits for review")),
        ("Merged", Some("Purple, until the workspace is archived")),
        ("Closed", Some("A muted red")),
        ("Draft", Some("A grey of its own")),
    ];
    asked(MARK, "Mark", true, &options)
}

fn refresh() -> Question {
    asked(REFRESH, "Refresh", false, &[("Every minute", None), ("Every five minutes", None)])
}

/// The question `text`, named `header`, offering `options`, each with what it means, if it says, of which several can
/// be chosen if `several`, with any other answer typed instead.
fn asked(text: &str, header: &str, several: bool, options: &[(&str, Option<&str>)]) -> Question {
    let option = |(label, description): &(&str, Option<&str>)| QuestionOption {
        label: (*label).into(),
        description: description.map(Into::into),
        preview: None,
    };
    let choose = if several { Choose::Several } else { Choose::One };
    Question {
        text: text.into(),
        header: Some(header.into()),
        options: options.iter().map(option).collect(),
        choose,
        typed: TypedAnswer::Yes,
    }
}

/// An answer that picked the options at `picked`, in that order, and typed `typed`, if anything.
fn picked(picked: &[u32], typed: Option<&str>) -> QuestionAnswer {
    QuestionAnswer { picked: picked.to_vec(), typed: typed.map(str::to_string) }
}

/// The agent asking `questions`, in the message `message`, as Claude's adapter asks them.
fn asking(message: &str, call: &str, approval: &str, questions: Vec<Question>) -> Vec<EventBody> {
    vec![
        typed_call(message, call, Call::Questions),
        ApprovalRequest {
            tool_use_id: Some(ToolUseId::from(call)),
            ..approval_request(approval, Asks::Questions { questions })
        }
        .into(),
    ]
}

/// The `answers` to `questions`, which the call `call` asked, one to each, as the hub records them, and as the agent
/// is told them.
fn answer(call: &str, approval: &str, questions: &[Question], answers: Vec<QuestionAnswer>) -> Vec<EventBody> {
    let said = |(question, answer): (&Question, &QuestionAnswer)| {
        let said: Vec<&str> = question.said(answer).collect();
        (!said.is_empty()).then(|| format!("\"{}\"=\"{}\"", question.text, said.join(", ")))
    };
    let told: Vec<String> = questions.iter().zip(&answers).filter_map(said).collect();
    let told = format!(
        "Your questions have been answered: {}. You can now continue with these answers in mind.",
        told.join(", ")
    );
    vec![decided(approval, Decision::Answer { answers }), said_to(call, &told)]
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
