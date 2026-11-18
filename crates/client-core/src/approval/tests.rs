use protocol::capability::asking::{PlanContext, QuestionAnswer, TypedAnswer};
use protocol::capability::{Capabilities, approvals::Decision};
use protocol::event::EventBody;
use protocol::fixtures::{allow_once, approval_request, asking, cancelled, decided, deny, plan, question, to_run};
use protocol::model::{RunCapabilities, RunId};

use super::{Answered, Approval, claims_of_the_asker};

fn answer(picked: &[u32], typed: Option<&str>) -> QuestionAnswer {
    QuestionAnswer { picked: picked.to_vec(), typed: typed.map(str::to_string) }
}

#[test]
fn questions_are_answered_by_position_and_refused_as_an_empty_answer_to_each() {
    let asks = asking(vec![question("Red or blue?", &["Red", "Blue"], false), question("Why?", &[], false)]);
    let body: EventBody = approval_request("a", asks.clone()).into();
    let approval = Approval::of(&body).unwrap();

    assert_eq!(approval.asks(), &asks);
    let answers = vec![answer(&[1], None), QuestionAnswer::default()];
    assert_eq!(approval.answered(answers.clone()), Decision::Answer { answers });
    let none_answered = Decision::Answer { answers: vec![QuestionAnswer::default(); 2] };
    assert_eq!(approval.refuse(None), none_answered);
    assert_eq!(approval.refuse(Some("Not now".into())), none_answered, "which takes no words");
    assert_eq!(approval.approve(), None, "a yes answers no question");
    assert_eq!(Approval::of(&EventBody::Unknown), None, "only a request asks");
}

#[test]
fn a_call_is_approved_this_once_or_refused_in_the_persons_words_or_the_adapters() {
    let body: EventBody = approval_request("a", to_run("ls")).into();
    let approval = Approval::of(&body).unwrap();

    assert_eq!(approval.approve(), Some(allow_once()));
    assert_eq!(approval.refuse(None), deny(None));
    assert_eq!(approval.refuse(Some("Use -la".into())), deny(Some("Use -la")));
}

#[test]
fn a_question_takes_a_typed_answer_where_it_says_and_keeps_a_secret_one_to_the_agent() {
    let typed = |typed| protocol::capability::asking::Question { typed, ..question("Token?", &[], false) };
    let asks = asking(vec![typed(TypedAnswer::Yes), typed(TypedAnswer::Secret), typed(TypedAnswer::No)]);
    let body: EventBody = approval_request("a", asks).into();
    let approval = Approval::of(&body).unwrap();

    assert_eq!((0..4).map(|at| approval.can_type(at)).collect::<Vec<_>>(), [true, true, false, false]);
    assert_eq!((0..4).map(|at| approval.secret(at)).collect::<Vec<_>>(), [false, true, false, false]);
}

#[test]
fn a_plan_is_approved_in_the_conversation_it_was_planned_in_or_kept_to() {
    let body: EventBody = approval_request("a", plan("1. Read")).into();
    let approval = Approval::of(&body).unwrap();

    assert_eq!(approval.approve(), Some(Decision::ApprovePlan { context: PlanContext::Kept }));
    assert_eq!(approval.refuse(None), Decision::KeepPlanning { words: None });
    assert_eq!(approval.refuse(Some("Smaller".into())), Decision::KeepPlanning { words: Some("Smaller".into()) });
}

/// The run that asked is the one going, whose claims the session's record has, or one that's gone, whose kind's are as
/// near as anything says.
#[test]
fn a_decision_goes_by_the_claims_of_the_run_that_asked_or_else_its_kinds() {
    let running = RunCapabilities { run: RunId::new(2), claims: Box::new(Capabilities::default()) };
    let mut kind = Capabilities::default();
    kind.asking.questions = Some(protocol::capability::Claimed {});

    assert_eq!(claims_of_the_asker(Some(&running), Some(RunId::new(2)), Some(&kind)), Some(&*running.claims));
    assert_eq!(claims_of_the_asker(Some(&running), Some(RunId::new(1)), Some(&kind)), Some(&kind), "one that's gone");
    assert_eq!(claims_of_the_asker(None, Some(RunId::new(2)), Some(&kind)), Some(&kind), "with none running");
    assert_eq!(claims_of_the_asker(Some(&running), None, None), None, "neither known");
}

#[test]
fn what_was_answered_is_read_from_the_decision_that_closed_it() {
    let days = question("Which days?", &["Monday", "Friday"], true);

    let answered = Answered::of(&decided("a", Decision::Answer { answers: vec![answer(&[1, 0], None)] })).unwrap();

    assert_eq!(answered.to(0, &days).as_deref(), Some("Friday, Monday"), "in the order it was picked");
    assert_eq!(answered.to(1, &days), None, "a question past the answers wasn't answered");
    assert_eq!(Answered::of(&decided("a", allow_once())), None, "a decision that answers nothing");
    assert_eq!(Answered::of(&cancelled("a")), None, "nor does nobody deciding");
}

#[test]
fn an_answer_in_words_chooses_the_options_it_names_and_is_otherwise_what_was_typed() {
    let asks = asking(vec![
        question("Red or blue?", &["Red", "Blue"], false),
        question("Which days?", &["Monday", "Wednesday", "Friday"], true),
    ]);
    let body: EventBody = approval_request("a", asks).into();
    let approval = Approval::of(&body).unwrap();
    let answers = |answers| Some(Decision::Answer { answers });
    let left = QuestionAnswer::default;

    assert_eq!(approval.answered_in_words("Red or blue?", "Blue"), answers(vec![answer(&[1], None), left()]));
    assert_eq!(
        approval.answered_in_words("Which days?", "Monday, Friday"),
        answers(vec![left(), answer(&[0, 2], None)])
    );
    assert_eq!(
        approval.answered_in_words("Which days?", "Monday, Sunday"),
        answers(vec![left(), answer(&[], Some("Monday, Sunday"))]),
        "words that name something it doesn't offer"
    );
    assert_eq!(
        approval.answered_in_words("Red or blue?", "Red, Blue"),
        answers(vec![answer(&[], Some("Red, Blue")), left()]),
        "only one of these"
    );
    assert_eq!(approval.answered_in_words("Why?", "Because"), None, "a question it doesn't ask");
}
