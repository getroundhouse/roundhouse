//! A pull request with something in every field of every facet, as every peer writes it, which what carries one
//! spells out.

use protocol::model::{ChecksRollup, LeftQueue, PullRequest, PullRequestNumber, QueueState, ReferenceLabel, Review};

/// Pull request #7 of example/project with something in every field of every facet, so each is written out.
pub(super) fn every_facet() -> PullRequest {
    let mut pull_request = protocol::fixtures::pull_request(7);
    let summary = &mut pull_request.summary.said;
    summary.about.author = Some("ada".into());
    summary.about.labels = vec![ReferenceLabel { name: "ui".into(), color: "0e8a16".into() }];
    (summary.review, summary.queue, summary.rollup) =
        (Review::Approved, Some(QueueState::AwaitingChecks), Some(ChecksRollup::Running));
    let history = &mut pull_request.history.as_mut().unwrap().said;
    history.changes_requested_at_ms = Some(3);
    history.left_queue = Some(LeftQueue { at_ms: 4, reason: "FAILED_CHECKS".into() });
    pull_request.stack.as_mut().unwrap().said.dependencies = vec![PullRequestNumber::try_from(6).unwrap()];
    pull_request
}

/// [`every_facet`], as every peer writes it, and the cache and the store keep it.
pub(super) const EVERY_FACET: &str = concat!(
    r#"{"repository":"example/project","number":7,"summary":{"said":{"about":{"#,
    r#""url":"https://github.com/example/project/pull/7","title":"Change 7","author":"ada","opened_at_ms":1,"#,
    r#""comments":0,"labels":[{"name":"ui","color":"0e8a16"}],"body":""},"state":"OPEN","is_draft":false,"#,
    r#""review":"APPROVED","queue":"AWAITING_CHECKS","merge_state":"CLEAN","mergeable":"mergeable","#,
    r#""base_branch":"main","head":"head-7","base":"base","added":0,"removed":0,"rollup":"running"},"#,
    r#""read_at_ms":1},"history":{"said":{"changes_requested_at_ms":3,"#,
    r#""left_queue":{"at_ms":4,"reason":"FAILED_CHECKS"}},"read_at_ms":1},"#,
    r#""stack":{"said":{"destination":"main","dependencies":[6],"linear":true},"read_at_ms":1},"#,
    r#""checks":{"said":{"revision":"head-7","requirements_known":true,"checks":[{"name":"checks","#,
    r#""source":"app:1","url":null,"state":"pending","required":true}]},"read_at_ms":1}}"#
);
