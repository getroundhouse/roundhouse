//! The rule's rows for a chat's proposals: made again and revised again by the same ask, and answered again by the
//! same request.

use protocol::fixtures;
use protocol::model::{Choice, ClientRequestId, OptionPlace, Proposal, Rev, RevisionNumber};
use protocol::prompt::Prompt;

use super::super::{AnswerToKeep, Answering, Made, NewRevision, Store};
use super::rule::Said;

pub(super) fn proposal(store: &Store) -> Rev {
    store.get_proposal(&"p".into()).unwrap().rev
}

/// As a new hub is asked whatever the last didn't answer.
pub(super) fn propose(store: &Store) -> Said {
    let (Made::New(proposal) | Made::Earlier(proposal)) =
        store.insert_proposal(&fixtures::proposal("p"), &"k".into()).unwrap();
    Said::record(&proposal)
}

pub(super) fn proposed(store: &Store) {
    propose(store);
}

/// As a client sends its answer again after a lost reply.
pub(super) fn answer(store: &Store) -> Said {
    let answered = Choice { chosen: vec![OptionPlace(1)], note: String::new(), at_ms: 5 };
    let (request, told) = (ClientRequestId::from("r"), |_: &Proposal, _: &Choice| Prompt::new("I chose B"));
    let (answered, request, told) = (&answered, &request, &told);
    let keep = AnswerToKeep { revision: RevisionNumber::FIRST, choice: answered, request, message: "m".into(), told };
    match store.answer_proposal(&"p".into(), keep).unwrap() {
        Answering::Kept { proposal, .. } | Answering::Again(proposal) => Said::record(&proposal),
        Answering::Otherwise(_) | Answering::Revised(_) | Answering::Refused(_) => panic!("not kept"),
    }
}

/// As a new hub is asked whatever the last didn't answer.
pub(super) fn revise(store: &Store) -> Said {
    let proposed = fixtures::proposal("p").current.proposed;
    let revision =
        NewRevision { changed: "Dropped B.".into(), call: None, proposed, made_at_ms: 6, images: Vec::new() };
    let (Made::New(proposal) | Made::Earlier(proposal)) =
        store.revise_proposal(&"p".into(), &revision, &"k2".into()).unwrap();
    Said::record(&proposal)
}
