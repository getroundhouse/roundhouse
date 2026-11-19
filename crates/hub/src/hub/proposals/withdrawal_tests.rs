use protocol::client::ErrorCode;
use protocol::client::proposals::ProposalAnswer;
use protocol::model::{
    AnswerProblem, OptionPlace, ProposalId, RevisionNumber, RevisionStanding, ToolUseId, Withdrawal,
};
use protocol::worker::{Revise, Withdraw};

use super::tests::asking;
use crate::hub::tests::hub_in;
use crate::store::now_ms;

fn withdrawing(id: &ProposalId, why: &str) -> Withdraw {
    Withdraw { id: id.clone(), why: why.into(), call: Some("toolu_9".into()) }
}

fn answer(id: &ProposalId) -> ProposalAnswer {
    ProposalAnswer {
        id: id.clone(),
        revision: RevisionNumber::FIRST,
        chosen: vec![OptionPlace(0)],
        note: String::new(),
        client_request_id: "r".into(),
    }
}

/// The agent withdraws a proposal it won't use, saying why, by the call the chat draws the line for, and then nothing
/// more can be said to it: an answer is refused, and so is a revision. Asked again, it's left as it was withdrawn.
#[tokio::test]
async fn a_withdrawn_proposal_takes_no_answer_or_revision() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;

    let withdrawn = hub.withdraw(&"s".into(), withdrawing(&made.id, " Dispatch decides now. ")).await.unwrap().proposal;
    let again = hub.withdraw(&"s".into(), withdrawing(&made.id, "Again")).await.unwrap().proposal;
    let answered = hub.answer_proposal(answer(&made.id)).await.unwrap_err();
    let revise = Revise {
        id: made.id.clone(),
        proposed: asking("k").proposed,
        changed: "More".into(),
        call: None,
        key: "k2".into(),
        images: Vec::new(),
        attachments: Vec::new(),
    };
    let revised = hub.revise(&"s".into(), revise).await.unwrap_err();

    let Some(Withdrawal { why, call, .. }) = &withdrawn.withdrawn else { panic!("{withdrawn:?}") };
    assert_eq!((why.as_str(), call.as_ref().map(ToolUseId::as_str)), ("Dispatch decides now.", Some("toolu_9")));
    assert!(matches!(withdrawn.standing(), RevisionStanding::Withdrawn(_)));
    assert!(withdrawn.rev > made.rev, "the feed tells clients of it");
    assert_eq!(again, withdrawn, "as it was withdrawn the first time");
    assert_eq!(
        (answered.code, answered.message.as_str()),
        (ErrorCode::Conflict, AnswerProblem::Withdrawn.to_string().as_str())
    );
    assert_eq!(revised.code, ErrorCode::Conflict);
    assert_eq!(hub.session_proposals("s".into()).await.unwrap().proposals, [withdrawn], "and it's read back so");
}

/// The agent says why, which the person reads, withdraws only what it proposed in its chat, and can't withdraw one the
/// person has chosen in, which is settled.
#[tokio::test]
async fn a_withdrawal_without_why_of_another_chats_or_once_chosen_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let made = hub.propose(&"w".into(), "s".into(), asking("k")).await.unwrap().proposal;
    let other = crate::store::tests::idle_session(now_ms());
    let other =
        crate::store::StoredSession { session: protocol::model::Session { id: "t".into(), ..other.session }, ..other };
    hub.store.insert_session(&other, None).unwrap();

    let unsaid = hub.withdraw(&"s".into(), withdrawing(&made.id, " ")).await.unwrap_err();
    let elsewhere = hub.withdraw(&"t".into(), withdrawing(&made.id, "Gone")).await.unwrap_err();
    hub.answer_proposal(answer(&made.id)).await.unwrap();
    let chosen = hub.withdraw(&"s".into(), withdrawing(&made.id, "Gone")).await.unwrap_err();

    assert_eq!(
        (unsaid.code, elsewhere.code, chosen.code),
        (ErrorCode::InvalidRequest, ErrorCode::NotFound, ErrorCode::Conflict)
    );
    assert_eq!(hub.session_proposals("s".into()).await.unwrap().proposals[0].withdrawn, None, "nothing withdrawn");
}
