use std::time::Duration;

use protocol::attachment::{Attachment, AttachmentBytes, MediaType, NewAttachment};
use protocol::client::proposals::{ProposalRevisions, ProposalRevisionsRead};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::LinkedImage;
use protocol::fixtures;
use protocol::model::{Proposed, RevisionNumber, ToolUseId};
use protocol::worker::Propose;

use crate::attachment_files::FileFor;

use crate::hub::tests::hub_in;
use crate::store::now_ms;

/// Long enough for the feed to hear of a change already made, on a loaded machine.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn asking(key: &str) -> Propose {
    Propose {
        proposed: fixtures::proposal("p").current.proposed,
        call: Some("toolu_1".into()),
        key: key.into(),
        images: Vec::new(),
        attachments: Vec::new(),
    }
}

/// An ask for a proposal that points to the drawing `address`, whose bytes the worker sent under `id`.
fn pointing(key: &str, address: &str, id: &str) -> Propose {
    let attachment = Attachment { id: id.into(), name: "flow.svg".into(), media_type: MediaType::Svg, size_bytes: 6 };
    let image = LinkedImage { address: address.into(), attachment: attachment.clone() };
    let bytes = NewAttachment { attachment, data: AttachmentBytes::new(b"<svg/>".to_vec()) };
    Propose { images: vec![image], attachments: vec![bytes], ..asking(key) }
}

/// What the agent wrote points to an image in its worktree, which the hub keeps as the chat's, as it does a reply's,
/// and names on the revision, so every client draws it as it was. Asked again, it keeps nothing more.
#[tokio::test]
async fn the_images_a_proposal_points_to_are_kept_with_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    let made = hub.propose(&"w".into(), "s".into(), pointing("k", "docs/flow.svg", "a1")).await.unwrap().proposal;
    let again = hub.propose(&"w".into(), "s".into(), pointing("k", "docs/flow.svg", "a2")).await.unwrap().proposal;

    let named: Vec<_> =
        made.current.images.iter().map(|image| (image.address.as_str(), image.attachment.id.as_str())).collect();
    assert_eq!(named, [("docs/flow.svg", "a1")]);
    assert_eq!(again, made, "the one made the first time");
    let kept = hub.file_read(FileFor::Session("s".into()), "a1".into()).await.unwrap();
    assert_eq!(kept.as_slice(), b"<svg/>");
    assert!(hub.file_read(FileFor::Session("s".into()), "a2".into()).await.is_err(), "nothing kept the second time");
    let listed = hub.session_records("s".into()).await.unwrap().proposals;
    assert_eq!(listed[0].current.images, made.current.images, "and it's read back from the store");
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
            if let Ok(HubNotice::Proposal(proposal)) = feed.recv().await.map(std::sync::Arc::unwrap_or_clone) {
                return proposal;
            }
        }
    });
    assert_eq!(told.await.unwrap(), made);
    let listed = hub.session_records("s".into()).await.unwrap();
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
    assert!(hub.session_records("s".into()).await.unwrap().proposals.is_empty());
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

    let read = hub.read_proposal(&"s".into(), made.id.clone()).await.unwrap();
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
