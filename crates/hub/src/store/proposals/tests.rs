use protocol::fixtures;
use protocol::model::{AnswerProblem, Choice, ClientRequestId, OptionPlace, Proposal, Response, RevisionNumber};
use protocol::prompt::{Prompt, PromptFrom};

use super::super::{Made, Store, StoreError, lock};
use super::{AnswerToKeep, Answering, NewRevision};
use crate::store::tests::store_with_session;

/// Writes the revisions from 2 to `newest` of the proposal `p`, each titled by its number.
fn revise_to(store: &Store, newest: u32) {
    for number in 2..=newest {
        let mut proposed = fixtures::proposal("p").current.proposed;
        proposed.title = format!("Revision {number}");
        let revision = NewRevision { changed: "More".into(), call: None, proposed, made_at_ms: 0, images: Vec::new() };
        store.revise_proposal(&"p".into(), &revision, &format!("k{number}").into()).unwrap();
    }
}

fn made(made: Made<Proposal>) -> (Proposal, bool) {
    match made {
        Made::New(proposal) => (proposal, true),
        Made::Earlier(proposal) => (proposal, false),
    }
}

#[test]
fn a_proposal_is_kept_once_for_its_ask_and_listed_by_its_chat_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let first = fixtures::proposal("p1");
    let second = fixtures::proposal("p2");

    let (kept, new) = made(store.insert_proposal(&first, &"k1".into()).unwrap());
    let (again, again_new) = made(store.insert_proposal(&fixtures::proposal("p3"), &"k1".into()).unwrap());
    store.insert_proposal(&second, &"k2".into()).unwrap();

    assert!(new && !again_new, "the same ask makes it once");
    assert_eq!(again, kept);
    assert!(kept.rev > protocol::model::Rev::default(), "it has a revision of its own");
    let (listed, rev) = store.list_proposals(&"s".into()).unwrap();
    let ids: Vec<_> = listed.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["p1", "p2"]);
    assert!(rev >= listed[1].rev);
    assert_eq!(store.get_proposal(&"p1".into()).unwrap(), kept);
}

/// Kept with the prompt that tells its agent, so an answer is never kept that its agent won't be told of. The hub wrote
/// the prompt, for the proposal, and gave it an ID of its own, apart from the request's.
#[test]
fn a_proposal_is_answered_once_with_its_prompt_queued_in_its_chat() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let (proposed, _) = made(store.insert_proposal(&fixtures::proposal("p"), &"k".into()).unwrap());
    let answered = Response::Chose(Choice { chosen: vec![OptionPlace(1)], note: "Keep it small".into(), at_ms: 5 });
    let told = |_: &Proposal, _: &Response| Prompt::new("I chose B");
    let queue = || -> Vec<(String, PromptFrom)> {
        let session = store.get_session(&"s".into()).unwrap().session;
        session.queued.iter().map(|queued| (queued.client_msg_id.to_string(), queued.from.clone())).collect()
    };
    let told_of_p = || {
        vec![("m1".to_string(), PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber::FIRST })]
    };
    // The proposal it left, and whether it kept this answer: just now, from the same request before, or not at all.
    let answering = |request: &str, message: &str| {
        let request = ClientRequestId::from(request);
        let (answered, request, message, told) = (&answered, &request, message.into(), &told);
        let keep = AnswerToKeep { revision: RevisionNumber::FIRST, response: answered, request, message, told };
        match store.answer_proposal(&"p".into(), keep).unwrap() {
            Answering::Kept { proposal, .. } => (proposal, "new"),
            Answering::Again(proposal) => (proposal, "again"),
            Answering::Otherwise(proposal) => (proposal, "otherwise"),
            Answering::Revised(proposal) => (proposal, "revised"),
            Answering::Refused(problem) => panic!("refused: {problem}"),
        }
    };

    let (kept, new) = answering("r1", "m1");
    let (again, same) = answering("r1", "m2");
    let (otherwise, other) = answering("r2", "m3");

    assert_eq!((new, same, other), ("new", "again", "otherwise"), "it's answered once");
    assert_eq!(kept.current.response.as_ref(), Some(&answered));
    assert_eq!((again, otherwise), (kept.clone(), kept.clone()), "another answer leaves the first");
    assert!(kept.rev > proposed.rev, "the feed tells clients of it");
    assert_eq!(queue(), told_of_p(), "its prompt queued once, by the hub's ID for it, from the proposal's answer");
    let request = ClientRequestId::from("r3");
    let (answered, request, told) = (&answered, &request, &told);
    let keep =
        AnswerToKeep { revision: RevisionNumber::FIRST, response: answered, request, message: "m4".into(), told };
    assert!(store.answer_proposal(&"nothing".into(), keep).is_err());
    assert_eq!(queue(), told_of_p());
}

/// Taking the prompt that tells the agent out of the queue takes the answer back with it, from the revision it
/// answered, so no answer is kept that its agent won't be told of, and the proposal waits for the person again, for
/// the same request or another to answer.
#[test]
fn an_answer_whose_prompt_is_taken_out_of_the_queue_is_taken_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.insert_proposal(&fixtures::proposal("p"), &"k".into()).unwrap();
    revise_to(&store, 2);
    let answered = Response::Chose(Choice { chosen: vec![OptionPlace(1)], ..answered() });
    let told = |_: &Proposal, _: &Response| Prompt::new("I chose B");
    let request = ClientRequestId::from("r1");
    let answer = |message: &str| AnswerToKeep {
        revision: RevisionNumber(2),
        response: &answered,
        request: &request,
        message: message.into(),
        told: &told,
    };
    let Answering::Kept { proposal: kept, .. } = store.answer_proposal(&"p".into(), answer("m1")).unwrap() else {
        panic!("answered")
    };

    let taken = store.unqueue_prompt(&"s".into(), &"m1".into()).unwrap();

    let back = taken.taken_back.expect("its answer, taken back");
    assert_eq!((back.current.number, back.current.response), (RevisionNumber(2), None));
    assert!(back.rev > kept.rev, "the feed tells clients of it");
    assert!(taken.reading.session.session.queued.is_empty());
    let again = store.answer_proposal(&"p".into(), answer("m2")).unwrap();
    assert!(matches!(again, Answering::Kept { .. }), "the same request answers it afresh");
}

/// A chat's list stays as small however often its proposals are revised, and the revisions before the newest are read
/// a page at a time, oldest first.
#[test]
fn only_a_proposals_newest_revision_is_listed_and_the_rest_are_read_a_page_at_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let (first, _) = made(store.insert_proposal(&fixtures::proposal("p"), &"k".into()).unwrap());

    revise_to(&store, 5);

    let (listed, _) = store.list_proposals(&"s".into()).unwrap();
    let newest = &listed[0];
    assert_eq!((newest.current.number, newest.current.proposed.title.as_str()), (RevisionNumber(5), "Revision 5"));
    assert!(newest.rev > first.rev, "the feed's copy is newer");
    let numbers = |from, limit| {
        let (revisions, newest) = store.proposal_revisions(&"p".into(), RevisionNumber(from), limit).unwrap();
        (revisions.iter().map(|revision| revision.number.0).collect::<Vec<_>>(), newest.0)
    };
    assert_eq!(numbers(1, 2), (vec![1, 2], 5));
    assert_eq!(numbers(4, 8), (vec![4, 5], 5));
    assert_eq!(numbers(6, 8), (vec![], 5));
    let (revisions, _) = store.proposal_revisions(&"p".into(), RevisionNumber::FIRST, 1).unwrap();
    assert_eq!(revisions, [first.current], "each as it was written");
    let unknown = store.proposal_revisions(&"nothing".into(), RevisionNumber::FIRST, 8);
    assert!(matches!(unknown, Err(StoreError::NotFound { .. })));
}

/// So an answer is always to what the person read: once the agent revises, an answer to the revision before is left
/// unkept, and one to a revision there isn't is refused.
#[test]
fn only_the_newest_revision_can_be_answered() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.insert_proposal(&fixtures::proposal("p"), &"k".into()).unwrap();
    revise_to(&store, 2);
    let answered = Response::Chose(Choice { chosen: vec![OptionPlace(1)], ..answered() });
    let (request, told) = (ClientRequestId::from("r1"), |_: &Proposal, _: &Response| Prompt::new("I chose B"));
    let answer = |revision| {
        let keep = AnswerToKeep { revision, response: &answered, request: &request, message: "m1".into(), told: &told };
        store.answer_proposal(&"p".into(), keep)
    };

    let revised = answer(RevisionNumber::FIRST).unwrap();
    let unwritten = answer(RevisionNumber(3));
    let kept = answer(RevisionNumber(2)).unwrap();

    assert!(matches!(revised, Answering::Revised(_)), "the revision before isn't answered");
    assert!(matches!(unwritten, Err(StoreError::NotFound { .. })));
    let Answering::Kept { proposal, .. } = kept else { panic!("the newest is answered") };
    assert_eq!(proposal.current.response.as_ref(), Some(&answered));
    let (revisions, _) = store.proposal_revisions(&"p".into(), RevisionNumber::FIRST, 8).unwrap();
    assert_eq!(revisions[0].response, None, "the one before is left as it was");
    let queued = store.get_session(&"s".into()).unwrap().session.queued;
    let told_of = PromptFrom::ProposalResponse { proposal: "p".into(), revision: RevisionNumber(2) };
    assert_eq!(queued.iter().map(|queued| &queued.from).collect::<Vec<_>>(), [&told_of], "it names the revision");
}

fn answered() -> Choice {
    Choice { chosen: Vec::new(), note: String::new(), at_ms: 5 }
}

/// Checked in the commit that would keep it, against the revision as it's kept then, so an answer checked against an
/// older copy, such as one read before an answer was taken back, isn't kept unchecked.
#[test]
fn an_answer_that_doesnt_fit_the_revision_is_refused_and_nothing_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.insert_proposal(&fixtures::proposal("p"), &"k".into()).unwrap();
    let unfit = Response::Chose(Choice { chosen: vec![OptionPlace(1), OptionPlace(0)], ..answered() });
    let request = ClientRequestId::from("r1");
    let told = |_: &Proposal, _: &Response| Prompt::new("I chose B, and A");
    let keep = AnswerToKeep {
        revision: RevisionNumber::FIRST,
        response: &unfit,
        request: &request,
        message: "m1".into(),
        told: &told,
    };

    let refused = store.answer_proposal(&"p".into(), keep).unwrap();

    assert!(matches!(refused, Answering::Refused(AnswerProblem::Unchosen { questions: 1 })), "one question, two picks");
    assert_eq!(store.get_proposal(&"p".into()).unwrap().current.response, None);
    assert!(store.get_session(&"s".into()).unwrap().session.queued.is_empty());
}

/// So a hub that opens what a newer one wrote doesn't take a response it can't read for none, and keep another.
#[test]
fn a_response_this_build_cant_read_is_still_a_response() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.insert_proposal(&fixtures::proposal("p"), &"k".into()).unwrap();
    lock(&store.writer)
        .execute(
            "UPDATE proposal_revisions SET response = '{\"kind\":\"chose\",\"picked\":\"B\"}', response_request = 'r0'",
            [],
        )
        .unwrap();
    let choice = Response::Chose(Choice { chosen: vec![OptionPlace(1)], ..answered() });
    let request = ClientRequestId::from("r1");
    let told = |_: &Proposal, _: &Response| Prompt::new("I chose B");
    let keep = AnswerToKeep {
        revision: RevisionNumber::FIRST,
        response: &choice,
        request: &request,
        message: "m1".into(),
        told: &told,
    };

    let read = store.get_proposal(&"p".into()).unwrap();
    let answered = store.answer_proposal(&"p".into(), keep).unwrap();

    assert_eq!(read.current.response, Some(Response::Unknown));
    assert!(matches!(answered, Answering::Otherwise(_)), "not answered again over it");
}
