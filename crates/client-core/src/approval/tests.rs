use protocol::event::{ApprovalDecision, Asking, EventBody, Question, QuestionAnswer, QuestionOption};
use protocol::fixtures::{ApprovalRequest, approval_request};
use serde_json::{Value, json};

use super::{Answered, Approval};

fn requested(tool: &str, input: Value, asking: Option<Asking>) -> EventBody {
    ApprovalRequest { input, asking, ..approval_request("a", tool) }.into()
}

fn question(text: &str, labels: &[&str], multiple: bool) -> Question {
    let options = labels.iter().map(|label| QuestionOption { label: (*label).into(), description: None }).collect();
    Question { text: text.into(), header: None, options, multiple }
}

fn answer(picked: &[u32], typed: Option<&str>) -> QuestionAnswer {
    QuestionAnswer { picked: picked.to_vec(), typed: typed.map(str::to_string) }
}

#[test]
fn a_request_that_says_what_it_asks_is_answered_by_position_and_refused_in_the_adapters_words() {
    let asking = Asking::Questions { questions: vec![question("Red or blue?", &["Red", "Blue"], false)] };
    // Its input says otherwise, which a newer adapter is free to do, and it's what it says it asks that counts.
    let body = requested("AskUserQuestion", json!({}), Some(asking.clone()));
    let approval = Approval::of(&body).unwrap();

    assert_eq!(approval.asking(), &asking);
    let answers = vec![answer(&[1], None)];
    assert_eq!(approval.answered(answers.clone()), ApprovalDecision::AnswerByPosition { answers });
    assert_eq!(approval.refused(), ApprovalDecision::Deny { message: None, interrupt: false });
}

/// As a hub from before it typed what Claude's requests asked sends one recorded before they said.
#[test]
fn a_request_that_says_nothing_of_what_it_asks_is_asked_about_as_a_tool_this_build_cant_read() {
    let bash = requested("Bash", json!({"command": "ls"}), None);

    assert_eq!(Approval::of(&bash).unwrap().asking(), &Asking::Unknown);
    assert_eq!(Approval::of(&bash).unwrap().questions(), None);
    assert_eq!(Approval::of(&EventBody::Unknown), None, "only a request asks");
}

#[test]
fn what_was_answered_is_read_from_the_record_in_whichever_form_it_came() {
    let closed = |by_text, by_position| EventBody::ApprovalResolved {
        approval_id: "a".into(),
        outcome: protocol::event::ApprovalOutcome::Allowed,
        answers_by_text: by_text,
        answers_by_position: by_position,
        by_setting: false,
    };
    let days = question("Which days?", &["Monday", "Friday"], true);

    let by_position = Answered::of(&closed(None, Some(vec![answer(&[1, 0], None)]))).unwrap();
    let by_text = Answered::of(&closed(Some([("Which days?".into(), "Friday".into())].into()), None)).unwrap();

    assert_eq!(by_position.to(0, &days).as_deref(), Some("Friday, Monday"), "in the order it was picked");
    assert_eq!(by_position.to(1, &days), None, "a question past the answers wasn't answered");
    assert_eq!(by_text.to(0, &days).as_deref(), Some("Friday"));
    assert_eq!(by_text.to(0, &question("Why?", &[], false)), None);
    assert_eq!(Answered::of(&closed(None, None)), None, "a record from before the hub kept answers");
}
