use protocol::client::proposals::ProposalAnswer;
use protocol::client::{ErrorCode, HubNotice};
use protocol::model::{OptionPlace, Proposal, RevisionNumber};

use super::answers::told;
use super::tests::{TIMEOUT, asking};
use crate::hub::tests::hub_in;

fn answer(request: &str, chosen: u8, note: &str) -> ProposalAnswer {
    ProposalAnswer {
        id: "p".into(),
        revision: RevisionNumber::FIRST,
        chosen: vec![OptionPlace(chosen)],
        note: note.into(),
        client_request_id: request.into(),
    }
}

/// So every client of the chat shows what was chosen, and a request sent again after a lost reply answers once. That
/// the agent is told, once, is what the end-to-end tests check, with an agent to tell.
#[tokio::test]
async fn an_answer_is_kept_once_and_the_feed_is_told() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let answer = |request, chosen| answer(request, chosen, "  Keep it small ");
    let answer = |request, chosen| ProposalAnswer { id: made.id.clone(), ..answer(request, chosen) };
    let mut feed = hub.subscribe_feed();

    let answered = hub.answer_proposal(answer("r1", 1)).await.unwrap();
    let again = hub.answer_proposal(answer("r1", 1)).await.unwrap();
    let otherwise = hub.answer_proposal(answer("r2", 0)).await.unwrap_err();

    let kept = answered.current.choice().cloned().unwrap();
    assert_eq!((kept.chosen, kept.note.as_str()), (vec![OptionPlace(1)], "Keep it small"));
    assert_eq!(again, answered, "the same request reads it back");
    assert_eq!(otherwise.code, ErrorCode::Conflict, "another answer is refused");
    let told = tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(HubNotice::Proposal(proposal)) = feed.recv().await.map(std::sync::Arc::unwrap_or_clone) {
                return proposal;
            }
        }
    });
    assert_eq!(told.await.unwrap(), answered);
    assert_eq!(hub.session_proposals("s".into()).await.unwrap().proposals, [answered]);
}

#[tokio::test]
async fn an_answer_that_doesnt_choose_one_of_the_options_is_refused_and_nothing_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;

    let refused = hub.answer_proposal(ProposalAnswer { id: made.id.clone(), ..answer("r", 2, "") }).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::InvalidRequest);
    assert!(refused.message.contains("has no such option"), "{}", refused.message);
    assert_eq!(hub.session_proposals("s".into()).await.unwrap().proposals[0].current.response, None);
}

/// The agent is told what was chosen, and reads the note with its tool, so a long note isn't in its prompt.
#[tokio::test]
async fn the_agent_is_told_what_was_chosen_and_where_the_note_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let id = made.id.clone();
    let answered = |note: &str| -> Proposal {
        let mut proposal = made.clone();
        let choice = protocol::model::Choice { chosen: vec![OptionPlace(1)], note: note.into(), at_ms: 0 };
        proposal.current.response = Some(protocol::model::Response::Chose(choice));
        proposal
    };
    let said = |proposal: Proposal| told(&proposal, proposal.current.response.as_ref().unwrap());

    assert_eq!(said(answered("")), format!("I chose B, In each client, in your proposal \"Search\" ({id})."));
    assert!(said(answered("Small")).ends_with("I wrote a note with it, which get_proposal reads."));
    assert!(!said(answered("Small")).contains("Small"), "the note isn't in the prompt");
}
