use protocol::fixtures;
use protocol::model::{Choice, OptionPlace, Proposal, ProposalRevision, Recommendation, Response, RevisionNumber};
use serde_json::{Map, Value, json};

use super::*;

fn arguments(arguments: &Value) -> Map<String, Value> {
    json!({ "name": PROPOSE, "arguments": arguments }).as_object().unwrap().clone()
}

fn written(recommended: &Value) -> Value {
    json!({
        "title": "Search",
        "context": "Chats get long.",
        "questions": [{
            "header": "Index",
            "question": "Where does the index live?",
            "options": [
                { "label": "On the hub", "description": "One index", "body": "The hub keeps it." },
                { "label": "In each client", "description": "Kept apart", "body": "Each client keeps its own." },
            ],
            "recommended": recommended,
            "why": "Every client shows the same.",
        }],
    })
}

#[test]
fn a_recommendation_is_read_by_its_letter_in_either_case() {
    let read = proposed(&arguments(&written(&json!("a")))).unwrap();

    let question = &read.questions[0];
    assert_eq!(
        question.recommended,
        Some(Recommendation { option: OptionPlace(0), why: "Every client shows the same.".into() })
    );
    assert_eq!((question.options[1].label.as_str(), read.context.as_str()), ("In each client", "Chats get long."));
}

/// The agent reads why, and writes it again.
#[test]
fn what_doesnt_fit_the_schema_is_refused_saying_why() {
    let mut extra = written(&Value::Null);
    extra["questions"][0]["colour"] = json!("red");
    let mut lettered = written(&Value::Null);
    lettered["questions"][0]["options"][1]["letter"] = json!("C");

    let not_a_letter = proposed(&arguments(&written(&json!("first")))).unwrap_err();
    let not_an_option = proposed(&arguments(&written(&json!("C")))).unwrap_err();
    let unknown = proposed(&arguments(&extra)).unwrap_err();
    let none = proposed(&arguments(&Value::Null)).unwrap_err();
    let named = proposed(&arguments(&lettered)).unwrap_err();

    assert!(not_a_letter.contains("question 1") && not_a_letter.contains("\"first\""), "{not_a_letter}");
    assert!(not_an_option.contains("recommends option C, which it doesn't have"), "{not_an_option}");
    assert!(unknown.contains("colour"), "{unknown}");
    assert!(none.contains("don't fit"), "{none}");
    assert!(named.contains("only revise_proposal takes"), "{named}");
}

/// As `agy` adds `toolSummary` to every tool's arguments.
#[test]
fn an_argument_a_cli_adds_at_the_top_is_ignored() {
    let mut added = written(&Value::Null);
    added["toolSummary"] = json!("Proposing where the index lives");

    assert!(proposed(&arguments(&added)).is_ok());
}

#[test]
fn a_proposal_read_back_says_each_option_by_its_letter_and_what_was_recommended() {
    let mut proposal = fixtures::proposal("p1");
    proposal.current.proposed = proposed(&arguments(&written(&json!("B")))).unwrap();

    let text = describe(&proposal, None);

    assert!(text.starts_with("Proposal p1, \"Search\".\n\nChats get long."), "{text}");
    assert!(text.contains("Question 1, Index: Where does the index live?"), "{text}");
    assert!(text.contains("A. On the hub: One index\nThe hub keeps it."), "{text}");
    assert!(
        text.ends_with("You recommended B. Every client shows the same.\n\nThe person hasn't answered yet."),
        "{text}"
    );
}

/// The prompt says only what was chosen, or that another round is asked for, so the agent reads the note here.
#[test]
fn a_proposal_read_back_once_answered_says_what_was_chosen_and_the_note() {
    let mut proposal = fixtures::proposal("p1");
    let answered = |note: &str| {
        let choice =
            protocol::model::Choice { chosen: vec![protocol::model::OptionPlace(1)], note: note.into(), at_ms: 0 };
        Some(protocol::model::Response::Chose(choice))
    };
    proposal.current.response = answered("");
    let plain = describe(&proposal, None);
    proposal.current.response = answered("Keep it small.");
    let noted = describe(&proposal, None);

    assert!(plain.ends_with("The person chose B, In each client."), "{plain}");
    assert!(noted.ends_with("The person chose B, In each client, and wrote:\nKeep it small."), "{noted}");
    let round = protocol::model::Round { note: "Could the board own it?".into(), at_ms: 0 };
    proposal.current.response = Some(protocol::model::Response::AnotherRound(round));
    let again = describe(&proposal, None);
    assert!(
        again.ends_with(
            "The person asked for another round, without choosing, and wrote what to change:\nCould the board own it?"
        ),
        "{again}"
    );
    proposal.current.response = Some(protocol::model::Response::Unknown);
    let newer = describe(&proposal, None);
    assert!(newer.ends_with("in a way this version of Roundhouse can't describe."), "not waiting: {newer}");
}

/// Once revised, the agent reads which revision it's reading, what changed in it, and, for one it revised before the
/// person answered, that they didn't.
#[test]
fn a_revision_read_back_says_which_it_is_and_what_changed() {
    let first = fixtures::proposal("p1");
    let second = protocol::model::ProposalRevision {
        number: protocol::model::RevisionNumber(2),
        changed: "Dropped B.".into(),
        ..first.current.clone()
    };
    let proposal = protocol::model::Proposal { current: second, ..first.clone() };

    let newest = describe(&proposal, None);
    let earlier = describe(&proposal, Some(&first.current));

    assert!(
        newest.contains("This is revision 2, the newest.\n\nWhat changed since the revision before: Dropped B."),
        "{newest}"
    );
    assert!(newest.ends_with("The person hasn't answered yet."), "{newest}");
    assert!(earlier.contains("This is revision 1. The newest is revision 2."), "{earlier}");
    assert!(earlier.ends_with("You revised it before the person answered."), "{earlier}");
}

/// An answer to the first revision names none, so when it reaches the agent after it revised, reading the newest
/// points it to the revision the person answered, where the note is.
#[test]
fn the_newest_read_back_points_to_an_answer_to_an_earlier_revision() {
    let mut first = fixtures::proposal("p1");
    let choice = Choice { chosen: vec![OptionPlace(1)], note: "Keep it small.".into(), at_ms: 0 };
    first.current.response = Some(Response::Chose(choice));
    let second = ProposalRevision { number: RevisionNumber(2), response: None, ..first.current.clone() };
    let revisions = vec![first.current.summary(), second.summary()];
    let proposal = Proposal { current: second, revisions, ..first };

    let newest = describe(&proposal, None);

    assert!(
        newest.ends_with(
            "The person hasn't answered yet.\n\nThe person chose B, In each client, in revision 1, before you revised \
             it. If you haven't had their prompt for it yet, it's on its way, and get_proposal reads what they said, \
             with their note, given revision 1."
        ),
        "{newest}"
    );
}
