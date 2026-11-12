use protocol::event::{ApprovalDecision, QuestionAnswer};
use serde_json::{Value, json};

use super::{DENIED, KEEP_PLANNING, NOT_ANSWERED, Waiting, response};
use crate::claude::approvals::asking;

/// The CLI's input for two questions, the second of which takes several answers.
fn two_questions() -> Value {
    json!({"questions": [
        {"question": "Red or blue?", "options": [{"label": "Red"}, {"label": "Blue"}]},
        {"question": "Which days?", "multiSelect": true,
         "options": [{"label": "Monday"}, {"label": "Wednesday"}, {"label": "Friday"}]},
    ]})
}

fn waiting(tool: &str, input: Value) -> Waiting {
    Waiting { asking: asking(tool, &input), input }
}

fn answer(picked: &[u32], typed: Option<&str>) -> QuestionAnswer {
    QuestionAnswer { picked: picked.to_vec(), typed: typed.map(str::to_string) }
}

#[test]
fn answers_by_position_reach_the_cli_by_the_questions_text_in_the_tools_input() {
    let answers = vec![answer(&[1], None), answer(&[0, 2], Some("Sunday"))];

    let told =
        response(&ApprovalDecision::AnswerByPosition { answers }, Some(waiting("AskUserQuestion", two_questions())));

    assert_eq!(told["behavior"], "allow");
    assert_eq!(told["updatedInput"]["questions"], two_questions()["questions"], "the input goes back as it came");
    assert_eq!(
        told["updatedInput"]["answers"],
        json!({"Red or blue?": "Blue", "Which days?": "Monday, Friday, Sunday"})
    );
}

#[test]
fn a_question_left_without_an_answer_is_left_out_and_an_option_it_doesnt_offer_is_ignored() {
    let answers = vec![QuestionAnswer::default(), answer(&[9, 1], None)];

    let told =
        response(&ApprovalDecision::AnswerByPosition { answers }, Some(waiting("AskUserQuestion", two_questions())));

    assert_eq!(told["updatedInput"]["answers"], json!({"Which days?": "Wednesday"}));
}

#[test]
fn answers_by_text_go_to_the_cli_as_they_came() {
    let answers = [("Red or blue?".to_string(), "Teal".to_string())].into();

    let told = response(&ApprovalDecision::AnswerByText { answers }, Some(waiting("AskUserQuestion", two_questions())));

    assert_eq!(told["updatedInput"]["answers"], json!({"Red or blue?": "Teal"}));
}

#[test]
fn saying_no_without_words_tells_the_agent_what_fits_what_was_asked() {
    let no = ApprovalDecision::Deny { message: None, interrupt: false };
    let told = |waiting| response(&no, waiting)["message"].as_str().unwrap().to_string();

    assert_eq!(told(Some(waiting("AskUserQuestion", two_questions()))), NOT_ANSWERED);
    assert_eq!(told(Some(waiting("ExitPlanMode", json!({"plan": "1. Read"})))), KEEP_PLANNING);
    assert_eq!(told(Some(waiting("Bash", json!({"command": "ls"})))), DENIED);
    assert_eq!(told(None), DENIED, "a request this process doesn't know");
}

#[test]
fn the_persons_own_words_are_what_the_agent_is_told() {
    let no = ApprovalDecision::Deny { message: Some("Not on main".into()), interrupt: true };

    let told = response(&no, Some(waiting("Bash", json!({"command": "git push"}))));

    assert_eq!(told, json!({"behavior": "deny", "message": "Not on main", "interrupt": true}));
}

#[test]
fn allowing_sends_the_input_back_unless_its_replaced() {
    let allow = |updated_input| ApprovalDecision::Allow { updated_input, updated_permissions: None };
    let plan = || Some(waiting("ExitPlanMode", json!({"plan": "1. Read"})));

    assert_eq!(response(&allow(None), plan())["updatedInput"], json!({"plan": "1. Read"}));
    assert_eq!(
        response(&allow(Some(json!({"plan": "2. Write"}))), plan())["updatedInput"],
        json!({"plan": "2. Write"})
    );
    assert_eq!(response(&allow(None), None)["updatedInput"], json!({}));
}
