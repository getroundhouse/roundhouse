use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
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

/// Two questions as Claude's adapter sent them before it said what a request asks, the second taking several answers.
fn from_before() -> EventBody {
    let input = json!({"questions": [
        {"question": "Red or blue?", "header": "Color",
         "options": [{"label": "Red", "description": "Warm"}, {"label": "Blue"}]},
        {"question": "Which days?", "multiSelect": true, "options": [{"label": "Monday"}, {"label": "Friday"}]},
        {"header": "no question"},
    ]});
    requested("AskUserQuestion", input, None)
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

#[test]
fn a_request_from_before_is_read_from_its_tool_and_answered_by_text() {
    let body = from_before();
    let approval = Approval::of(&body).unwrap();

    let red = QuestionOption { label: "Red".into(), description: Some("Warm".into()) };
    let blue = QuestionOption { label: "Blue".into(), description: None };
    let colors = Question {
        text: "Red or blue?".into(),
        header: Some("Color".into()),
        options: vec![red, blue],
        multiple: false,
    };
    assert_eq!(approval.questions(), Some(&[colors, question("Which days?", &["Monday", "Friday"], true)][..]));
    let answers = vec![QuestionAnswer::default(), answer(&[0, 1], Some("Sunday"))];
    let by_text = [("Which days?".to_string(), "Monday, Friday, Sunday".to_string())].into();
    assert_eq!(approval.answered(answers), ApprovalDecision::AnswerByText { answers: by_text });
}

#[test]
fn a_request_from_before_is_refused_in_the_words_clients_used_then() {
    let refused = |body: EventBody| {
        let refused = Approval::of(&body).unwrap().refused();
        let ApprovalDecision::Deny { message: Some(message), interrupt: false } = refused else {
            panic!("{refused:?}")
        };
        message
    };

    assert!(refused(from_before()).starts_with("The user chose not to answer."));
    assert!(refused(requested("ExitPlanMode", json!({"plan": "1. Read"}), None)).contains("keep planning"));
    assert_eq!(refused(requested("Bash", json!({"command": "ls"}), None)), "The user denied this.");
}

#[test]
fn a_plan_from_before_is_its_text_and_any_other_tool_asks_to_be_used() {
    let plan = requested("ExitPlanMode", json!({"plan": "1. Read", "planFilePath": "/tmp/plan.md"}), None);
    let bash = requested("Bash", json!({"command": "ls"}), None);

    assert_eq!(Approval::of(&plan).unwrap().asking(), &Asking::Plan { plan: "1. Read".into() });
    assert_eq!(Approval::of(&bash).unwrap().asking(), &Asking::ToolUse { call: None });
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

/// Questions with texts and labels of their own, and an answer for each, some of them empty.
fn asked_and_answered() -> impl Strategy<Value = (Vec<Question>, Vec<QuestionAnswer>)> {
    vec((0..4usize, proptest::bool::ANY), 0..6).prop_flat_map(|shapes| {
        let questions: Vec<Question> = shapes
            .iter()
            .enumerate()
            .map(|(q, &(options, multiple))| {
                let labels = Vec::from_fn(options, |o| format!("Q{q} option {o}"));
                let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                question(&format!("Question {q}?"), &labels, multiple)
            })
            .collect();
        let answers: Vec<_> = shapes
            .iter()
            .map(|&(options, _)| {
                let picked = proptest::sample::subsequence(
                    (0..u32::try_from(options).unwrap()).collect::<Vec<_>>(),
                    0..=options,
                );
                (picked, proptest::option::of("[a-z]{1,4}"))
                    .prop_map(|(picked, typed)| QuestionAnswer { picked, typed })
            })
            .collect();
        (Just(questions), answers)
    })
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Sent by text to an adapter from before, the answers say the same of each question as they would by position,
    /// and a question with no answer is left out.
    #[test]
    fn answers_by_text_say_what_they_would_by_position((questions, answers) in asked_and_answered()) {
        let body = requested("AskUserQuestion", json!({}), None);
        let mut approval = Approval::of(&body).unwrap();
        approval.asking = std::borrow::Cow::Owned(Asking::Questions { questions: questions.clone() });

        let ApprovalDecision::AnswerByText { answers: by_text } = approval.answered(answers.clone()) else { panic!() };

        prop_assert_eq!(by_text.len(), answers.iter().filter(|answer| !answer.is_empty()).count());
        let (by_text, by_position) = (Answered::ByText(by_text), Answered::ByPosition(answers));
        for (at, question) in questions.iter().enumerate() {
            prop_assert_eq!(by_text.to(at, question), by_position.to(at, question));
        }
    }
}
