use protocol::client::ErrorCode;
use protocol::client::proposals::{ProposalAnotherRound, ProposalAnswer};
use protocol::model::{OptionPlace, ProposalId, Response, ResponseSummary, RevisionNumber, RevisionStanding, Round};
use protocol::worker::Revise;

use super::answers::told;
use super::tests::asking;
use crate::hub::tests::hub_in;

fn another_round(id: &ProposalId, request: &str, note: &str) -> ProposalAnotherRound {
    ProposalAnotherRound {
        id: id.clone(),
        revision: RevisionNumber::FIRST,
        note: note.into(),
        client_request_id: request.into(),
    }
}

fn revising(id: &ProposalId) -> Revise {
    Revise {
        id: id.clone(),
        proposed: asking("k").proposed,
        changed: "Added the board".into(),
        call: Some("toolu_2".into()),
        key: "toolu_2".into(),
        images: Vec::new(),
        attachments: Vec::new(),
    }
}

/// Another round is the person's response to the revision they read, kept once with its note, so the revision no
/// longer waits for them, and no choice can be given to it. The agent's next revision waits for them, and the record
/// sums up what was said to the one before.
#[tokio::test]
async fn another_round_is_kept_once_with_its_note_and_the_next_revision_waits() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let note = "  Could the board own it? ";

    let asked = hub.ask_another_round(another_round(&made.id, "r1", note)).await.unwrap();
    let again = hub.ask_another_round(another_round(&made.id, "r1", note)).await.unwrap();
    let choice = ProposalAnswer {
        id: made.id.clone(),
        revision: RevisionNumber::FIRST,
        chosen: vec![OptionPlace(0)],
        note: String::new(),
        client_request_id: "r2".into(),
    };
    let chose = hub.answer_proposal(choice).await.unwrap_err();
    let revised = hub.revise(&"s".into(), revising(&made.id)).await.unwrap().proposal;

    let RevisionStanding::AnotherRound(Round { note, .. }) = asked.current.standing() else { panic!("{asked:?}") };
    assert_eq!(note, "Could the board own it?");
    assert_eq!(again, asked, "the same request reads it back");
    assert_eq!(chose.code, ErrorCode::Conflict, "nothing's chosen in a revision that went another round");
    assert_eq!(revised.current.standing(), RevisionStanding::Waiting);
    let Some(ResponseSummary::AnotherRound { note, .. }) = &revised.revisions[0].response else {
        panic!("{revised:?}")
    };
    assert_eq!(note, "Could the board own it?");
}

/// The note is what tells the agent what to change, so another round isn't asked for without one.
#[tokio::test]
async fn another_round_without_a_note_is_refused_and_nothing_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;

    let refused = hub.ask_another_round(another_round(&made.id, "r", " \n ")).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::InvalidRequest);
    assert!(refused.message.contains("note"), "{}", refused.message);
    assert_eq!(hub.session_records("s".into()).await.unwrap().proposals[0].current.response, None);
}

/// The agent is told the person asks for another round, and reads what to change in the note with its tool, of the
/// revision they read once there are several.
#[tokio::test]
async fn the_agent_is_told_of_another_round_and_where_its_note_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let response = Response::AnotherRound(Round { note: "Could the board own it?".into(), at_ms: 0 });
    let revised = hub.revise(&"s".into(), revising(&made.id)).await.unwrap().proposal;

    let first = told(&made, &response);
    let second = told(&revised, &response);

    assert_eq!(
        first,
        format!(
            "I'd like another round of your proposal \"Search\" ({}), without choosing yet. My note says what to \
             change, which get_proposal reads. Then revise it with revise_proposal.",
            made.id
        )
    );
    assert!(!first.contains("board"), "the note isn't in the prompt");
    assert!(second.contains("another round of revision 2 of your proposal"), "{second}");
    assert!(second.contains("get_proposal reads, given revision 2"), "{second}");
}
