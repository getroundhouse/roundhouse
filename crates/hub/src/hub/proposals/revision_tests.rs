use protocol::client::ErrorCode;
use protocol::client::proposals::ProposalAnswer;
use protocol::model::{
    AnswerProblem, OptionLetter, OptionPlace, ResponseSummary, RevisionNumber, ToolUseId, chosen_in_words,
};
use protocol::worker::Revise;

use super::answers::told;
use super::tests::asking;
use crate::hub::tests::hub_in;
use crate::store::now_ms;

fn revising(id: &protocol::model::ProposalId, key: &str) -> Revise {
    let mut proposed = asking("k").proposed;
    proposed.questions[0].options[1].letter = OptionLetter::read("C");
    Revise {
        id: id.clone(),
        proposed,
        changed: " Dropped B. ".into(),
        call: Some(key.into()),
        key: key.into(),
        images: Vec::new(),
        attachments: Vec::new(),
    }
}

/// The agent revises a proposal the person has answered, or not, as a new revision, which says what changed, and the
/// record sums up each revision, so every client draws each one's line. Asked again, it writes nothing more.
#[tokio::test]
async fn a_revision_is_written_once_after_the_newest_and_the_record_sums_up_each() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let answer = ProposalAnswer {
        id: made.id.clone(),
        revision: RevisionNumber::FIRST,
        chosen: vec![OptionPlace(1)],
        note: "Not quite".into(),
        client_request_id: "r".into(),
    };
    hub.answer_proposal(answer).await.unwrap();

    let revised = hub.revise(&"s".into(), revising(&made.id, "toolu_2")).await.unwrap().proposal;
    let again = hub.revise(&"s".into(), revising(&made.id, "toolu_2")).await.unwrap().proposal;

    assert_eq!(again, revised, "the one written the first time");
    let current = &revised.current;
    assert_eq!(
        (current.number, current.changed.as_str(), current.response.as_ref()),
        (RevisionNumber(2), "Dropped B.", None)
    );
    let question = &current.proposed.questions[0];
    assert_eq!(question.letter(OptionPlace(1)), OptionLetter::read("C"), "the letter the agent gave it");
    let calls: Vec<_> =
        revised.revisions.iter().map(|r| (r.number.0, r.call.as_ref().map(ToolUseId::as_str))).collect();
    assert_eq!(calls, [(1, Some("toolu_1")), (2, Some("toolu_2"))]);
    let Some(ResponseSummary::Chose { chosen, note, .. }) = &revised.revisions[0].response else {
        panic!("{revised:?}")
    };
    assert_eq!((chosen_in_words(chosen), note.as_str()), ("B, In each client".to_string(), "Not quite"));
    assert_eq!(hub.session_records("s".into()).await.unwrap().proposals, [revised], "and it's read back so");
}

/// An answer to a revision the agent has since revised is refused, since the person hasn't read the newest.
#[tokio::test]
async fn an_answer_to_a_revision_since_revised_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    hub.revise(&"s".into(), revising(&made.id, "toolu_2")).await.unwrap();
    let answer = ProposalAnswer {
        id: made.id.clone(),
        revision: RevisionNumber::FIRST,
        chosen: vec![OptionPlace(0)],
        note: String::new(),
        client_request_id: "r".into(),
    };

    let refused = hub.answer_proposal(answer).await.unwrap_err();

    assert_eq!(refused.message, AnswerProblem::Revised { newest: RevisionNumber(2) }.to_string());
}

/// The agent says what changed, which the new revision shows first, and revises only what it proposed in its chat.
#[tokio::test]
async fn a_revision_without_what_changed_or_of_another_chats_proposal_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let other = crate::store::tests::idle_session(now_ms());
    let other =
        crate::store::StoredSession { session: protocol::model::Session { id: "t".into(), ..other.session }, ..other };
    hub.store.insert_session(&other, None).unwrap();

    let unsaid = hub.revise(&"s".into(), Revise { changed: " ".into(), ..revising(&made.id, "k2") }).await.unwrap_err();
    let elsewhere = hub.revise(&"t".into(), revising(&made.id, "k3")).await.unwrap_err();

    assert_eq!((unsaid.code, elsewhere.code), (ErrorCode::InvalidRequest, ErrorCode::NotFound));
    let listed = hub.session_records("s".into()).await.unwrap().proposals;
    assert_eq!(listed[0].current.number, RevisionNumber::FIRST, "nothing written");
}

/// Once there are several, the agent is told which revision the person answered, since it may have revised since.
#[tokio::test]
async fn the_answer_to_a_revision_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let mut revised = hub.revise(&"s".into(), revising(&made.id, "toolu_2")).await.unwrap().proposal;
    let choice = protocol::model::Choice { chosen: vec![OptionPlace(1)], note: String::new(), at_ms: 0 };
    let response = protocol::model::Response::Chose(choice);
    revised.current.response = Some(response.clone());

    let said = told(&revised, &response);

    assert_eq!(said, format!("I chose C, In each client, in revision 2 of your proposal \"Search\" ({}).", made.id));
}

/// The agent reads the revision an answer's prompt names in full, such as its note, after it's revised since.
#[tokio::test]
async fn the_agent_reads_an_earlier_revision_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    hub.revise(&"s".into(), revising(&made.id, "toolu_2")).await.unwrap();
    let read = |revision: u32| protocol::worker::ReadProposal {
        id: made.id.clone(),
        revision: Some(RevisionNumber(revision)),
    };

    let first = hub.read_proposal_revision(&"s".into(), read(1)).await.unwrap();
    let newest = hub.read_proposal_revision(&"s".into(), read(2)).await.unwrap();
    let missing = hub.read_proposal_revision(&"s".into(), read(3)).await.unwrap_err();

    assert_eq!(first.revision.as_deref(), Some(&made.current), "the first, as it was written");
    assert_eq!(
        (first.proposal.current.number, newest.revision),
        (RevisionNumber(2), None),
        "the newest is the record's"
    );
    assert_eq!(missing.code, ErrorCode::NotFound);
}
