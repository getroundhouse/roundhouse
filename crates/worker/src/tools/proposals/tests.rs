use protocol::fixtures;
use protocol::model::{OptionPlace, Recommendation};
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

    let not_a_letter = proposed(&arguments(&written(&json!("first")))).unwrap_err();
    let not_an_option = proposed(&arguments(&written(&json!("C")))).unwrap_err();
    let unknown = proposed(&arguments(&extra)).unwrap_err();
    let none = proposed(&arguments(&Value::Null)).unwrap_err();

    assert!(not_a_letter.contains("question 1") && not_a_letter.contains("\"first\""), "{not_a_letter}");
    assert!(not_an_option.contains("recommends option C, which it doesn't have"), "{not_an_option}");
    assert!(unknown.contains("colour"), "{unknown}");
    assert!(none.contains("don't fit"), "{none}");
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

    let text = describe(&proposal);

    assert!(text.starts_with("Proposal p1, \"Search\".\n\nChats get long."), "{text}");
    assert!(text.contains("Question 1, Index: Where does the index live?"), "{text}");
    assert!(text.contains("A. On the hub: One index\nThe hub keeps it."), "{text}");
    assert!(
        text.ends_with("You recommended B. Every client shows the same.\n\nThe person hasn't answered yet."),
        "{text}"
    );
}

/// The prompt says only what was chosen, so the agent reads the note here.
#[test]
fn a_proposal_read_back_once_answered_says_what_was_chosen_and_the_note() {
    let mut proposal = fixtures::proposal("p1");
    let answered = |note: &str| {
        let choice =
            protocol::model::Choice { chosen: vec![protocol::model::OptionPlace(1)], note: note.into(), at_ms: 0 };
        Some(protocol::model::Response::Chose(choice))
    };
    proposal.current.response = answered("");
    let plain = describe(&proposal);
    proposal.current.response = answered("Keep it small.");
    let noted = describe(&proposal);

    assert!(plain.ends_with("The person chose B, In each client."), "{plain}");
    assert!(noted.ends_with("The person chose B, In each client, and wrote:\nKeep it small."), "{noted}");
    proposal.current.response = Some(protocol::model::Response::Unknown);
    let newer = describe(&proposal);
    assert!(newer.ends_with("in a way this version of Roundhouse can't describe."), "not waiting: {newer}");
}
