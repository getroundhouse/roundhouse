use std::time::Duration;

use protocol::client::proposals::{ProposalRevisions, ProposalRevisionsRead};
use protocol::client::{ErrorCode, HubNotice};
use protocol::fixtures;
use protocol::model::{Proposed, RevisionNumber, ToolUseId};
use protocol::worker::Propose;

use crate::hub::tests::hub_in;
use crate::store::now_ms;

/// Long enough for the feed to hear of a change already made, on a loaded machine.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn asking(key: &str) -> Propose {
    Propose { proposed: fixtures::proposal("p").current.proposed, call: Some("toolu_1".into()), key: key.into() }
}

/// So every client of the chat shows it as it's made, and a worker asking again, as it asks each new hub, makes one.
#[tokio::test]
async fn a_proposal_is_made_once_for_its_ask_and_the_feed_is_told() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut feed = hub.subscribe_feed();

    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let again = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;

    assert_eq!(again, made);
    assert_eq!((made.session_id.as_str(), made.current.call.as_ref().map(ToolUseId::as_str)), ("s", Some("toolu_1")));
    let told = tokio::time::timeout(TIMEOUT, async {
        loop {
            if let Ok(HubNotice::Proposal(proposal)) = feed.recv().await {
                return proposal;
            }
        }
    });
    assert_eq!(told.await.unwrap(), made);
    let listed = hub.session_proposals("s".into()).await.unwrap();
    assert_eq!(listed.proposals, [made]);
}

/// The agent reads why, and writes it again.
#[tokio::test]
async fn a_proposal_that_doesnt_check_is_refused_saying_why_and_nothing_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let empty = Propose { proposed: Proposed { questions: Vec::new(), ..asking("k").proposed }, ..asking("k") };

    let refused = hub.propose(&"w".into(), "s".into(), empty).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::InvalidRequest);
    assert!(refused.message.contains("questions"), "{}", refused.message);
    assert!(hub.session_proposals("s".into()).await.unwrap().proposals.is_empty());
}

/// An agent reads only what was proposed in its own chat.
#[tokio::test]
async fn a_chat_reads_its_own_proposals_and_not_anothers() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let other = crate::store::tests::idle_session(now_ms());
    let other =
        crate::store::StoredSession { session: protocol::model::Session { id: "t".into(), ..other.session }, ..other };
    hub.store.insert_session(&other, None).unwrap();

    let read = hub.read_proposal(&"s".into(), made.id.clone()).await.unwrap().proposal;
    let refused = hub.read_proposal(&"t".into(), made.id.clone()).await.unwrap_err();

    assert_eq!(read, made);
    assert_eq!(refused.code, ErrorCode::NotFound);
}

/// However many a client asks for, a read sends at least one revision and no more than the most, so it's never empty
/// for nothing, and never larger than a few revisions.
#[tokio::test]
async fn a_proposals_revisions_are_read_at_least_one_and_at_most_the_most_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let id = made.id.clone();
    let read = |limit| ProposalRevisionsRead { id: id.clone(), from: RevisionNumber::FIRST, limit };

    let none_asked = hub.proposal_revisions(read(0)).await.unwrap();
    let unknown = ProposalRevisionsRead { id: "nothing".into(), ..read(1) };

    assert_eq!(none_asked, ProposalRevisions { revisions: vec![made.current], newest: RevisionNumber::FIRST });
    assert_eq!(hub.proposal_revisions(unknown).await.unwrap_err().code, ErrorCode::NotFound);
}
