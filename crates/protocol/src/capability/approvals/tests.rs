use super::RuleEffect;
use super::{Access, AfterDenial, Asks, Covers, Decision, Kept, Lasts, NetworkProtocol, Remember, Request, Rule};
use crate::capability::asking::{Choose, PlanContext, Question, QuestionAnswer, QuestionOption, TypedAnswer};
use crate::capability::calls::{Call, CallKind};
use crate::fixtures::typed_command;

fn asking(asks: Asks) -> Request {
    Request { asks, suggested: vec![], always: false, why: None }
}

fn rule(effect: RuleEffect) -> Rule {
    let covers = Covers::Calls { call: CallKind::Command };
    Rule { covers, matching: "npm test:*".into(), effect, kept: Kept::Conversation, described: None }
}

fn deny(keep: Option<Rule>) -> Decision {
    Decision::Deny { words: Some("no".into()), then: AfterDenial::Continue, keep }
}

fn question(options: usize, choose: Choose, typed: TypedAnswer) -> Question {
    let option = |n| QuestionOption { label: format!("Option {n}"), description: None, preview: None };
    Question { text: "Which?".into(), header: None, options: (0..options).map(option).collect(), choose, typed }
}

fn answer(picked: &[u32], typed: Option<&str>) -> QuestionAnswer {
    QuestionAnswer { picked: picked.to_vec(), typed: typed.map(str::to_string) }
}

#[test]
fn a_call_is_allowed_or_refused() {
    let suggested = vec![rule(RuleEffect::Allow), rule(RuleEffect::Deny)];
    let request = Request { suggested, ..asking(Asks::Call { call: Call::Command(typed_command("rm -rf build")) }) };
    let allow = |remember| Decision::Allow { remember };

    assert!(allow(Remember::Once).answers(&request));
    assert!(allow(Remember::Conversation).answers(&request));
    assert!(allow(Remember::Rule(rule(RuleEffect::Allow))).answers(&request));
    assert!(deny(Some(rule(RuleEffect::Deny))).answers(&request));
    for other in [Decision::Answer { answers: vec![] }, Decision::KeepPlanning { words: None }, Decision::Unknown] {
        assert!(!other.answers(&request), "{other:?}");
    }
}

#[test]
fn a_call_keeps_only_a_rule_it_suggests_wherever_the_person_keeps_it() {
    let request = Request { suggested: vec![rule(RuleEffect::Allow)], ..asking(Asks::Call { call: Call::Advisor }) };
    let keeping = |rule| Decision::Allow { remember: Remember::Rule(rule) }.answers(&request);

    assert!(keeping(Rule { kept: Kept::Project, ..rule(RuleEffect::Allow) }), "kept somewhere else");
    assert!(!keeping(Rule { matching: "*".into(), ..rule(RuleEffect::Allow) }), "wider than suggested");
    assert!(!keeping(Rule { covers: Covers::Calls { call: CallKind::Write }, ..rule(RuleEffect::Allow) }));
    assert!(!deny(Some(rule(RuleEffect::Deny))).answers(&request), "a denial it doesn't suggest keeping");
}

#[test]
fn a_request_that_comes_whatever_the_mode_keeps_no_rule() {
    let request = Request { always: true, ..asking(Asks::Call { call: Call::Advisor }) };

    assert!(Decision::Allow { remember: Remember::Conversation }.answers(&request));
    assert!(!Decision::Allow { remember: Remember::Rule(rule(RuleEffect::Allow)) }.answers(&request));
    assert!(deny(None).answers(&request));
    assert!(!deny(Some(rule(RuleEffect::Deny))).answers(&request));
}

#[test]
fn questions_are_answered_one_answer_each_as_each_allows_or_left_unanswered() {
    let questions = vec![question(3, Choose::One, TypedAnswer::No), question(2, Choose::Several, TypedAnswer::Yes)];
    let request = asking(Asks::Questions { questions });
    let answering = |answers: Vec<QuestionAnswer>| Decision::Answer { answers }.answers(&request);

    assert!(answering(vec![answer(&[2], None), answer(&[0, 1], Some("and more"))]));
    assert!(answering(vec![QuestionAnswer::default(), QuestionAnswer::default()]), "neither answered");
    assert!(!answering(vec![answer(&[2], None)]), "one answer for two questions");
    assert!(!answering(vec![answer(&[0, 1], None), QuestionAnswer::default()]), "two picked of one that takes one");
    assert!(!answering(vec![answer(&[3], None), QuestionAnswer::default()]), "an option it doesn't offer");
    assert!(!answering(vec![answer(&[], Some("mine")), QuestionAnswer::default()]), "typed where only a choice is");
    assert!(!deny(None).answers(&request), "none answered is an empty answer to each, which says it one way");
}

#[test]
fn a_question_takes_each_option_once_and_what_it_doesnt_know_allows_the_least() {
    let several = question(3, Choose::Several, TypedAnswer::Secret);

    assert!(several.takes(&answer(&[0, 2], Some("hunter2"))));
    assert!(!several.takes(&answer(&[1, 1], None)), "an option twice");
    let unknown = question(3, Choose::Unknown, TypedAnswer::Unknown);
    assert!(unknown.takes(&answer(&[1], None)));
    assert!(!unknown.takes(&answer(&[0, 1], None)), "several, where it may take one");
    assert!(!unknown.takes(&answer(&[], Some("mine"))), "typed, where it may not be");
}

#[test]
fn every_other_request_takes_only_the_decisions_of_its_own_kind() {
    let plan = asking(Asks::Plan { plan: "1. Test".into(), file: None });
    assert!(Decision::ApprovePlan { context: PlanContext::Kept }.answers(&plan));
    assert!(Decision::KeepPlanning { words: None }.answers(&plan));
    assert!(!deny(None).answers(&plan));

    let host = asking(Asks::Network { host: "example.com".into(), protocol: NetworkProtocol::Https });
    assert!(Decision::Allow { remember: Remember::Once }.answers(&host));

    let access = asking(Asks::Access { access: vec![Access::Read { path: "/srv".into() }, Access::Network] });
    let grant = |access| Decision::Grant { access, lasts: Lasts::Turn };
    assert!(grant(vec![Access::Network]).answers(&access));
    assert!(!grant(vec![Access::Write { path: "/srv".into() }]).answers(&access), "more than was asked");

    let unknown = asking(Asks::Unknown);
    assert!(deny(None).answers(&unknown), "refused, which its adapter knows how to say");
    assert!(!Decision::Allow { remember: Remember::Once }.answers(&unknown));
}

#[test]
fn a_decision_or_a_part_this_build_doesnt_know_is_never_sent_on() {
    let claims = crate::capability::Capabilities::default();

    assert!(!Decision::Unknown.allowed(&claims));
    assert!(!Decision::Unknown.answers(&asking(Asks::Unknown)));
    let stopping = |then| Decision::Deny { words: None, then, keep: None };
    assert!(!stopping(AfterDenial::Unknown).allowed(&claims), "denied, and then what this build doesn't know");
}
