//! A made-up proposal, for the tests of every crate.

use crate::model::{Proposal, ProposalOption, ProposalQuestion, ProposalRevision, Proposed, Rev, RevisionNumber};

/// The proposal `id`, in the session `s` of the workspace `w`, in its first revision, made by no call the hub knows,
/// deciding one thing between two options, with no context, details or recommendation.
pub fn proposal(id: &str) -> Proposal {
    let option = |label: &str| ProposalOption { label: label.into(), description: String::new(), body: String::new() };
    let question = ProposalQuestion {
        header: "Index".into(),
        text: "Where does the index live?".into(),
        details: String::new(),
        options: vec![option("On the hub"), option("In each client")],
        recommended: None,
    };
    Proposal {
        id: id.into(),
        workspace_id: "w".into(),
        session_id: "s".into(),
        current: ProposalRevision {
            number: RevisionNumber::FIRST,
            call: None,
            proposed: Proposed { title: "Search".into(), context: String::new(), questions: vec![question] },
            made_at_ms: 0,
            response: None,
            images: Vec::new(),
        },
        created_at_ms: 0,
        rev: Rev::default(),
    }
}
