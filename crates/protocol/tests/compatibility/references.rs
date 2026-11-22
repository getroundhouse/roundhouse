//! A client's read of the issues and pull requests a chat links to, and the hub's answer, which every peer from now on
//! reads as they were first written.

use protocol::client::references::{References, ReferencesRead};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{
    About, ChecksRollup, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, MergeState, Mergeability,
    PrState, PullRequest, PullRequestNumber, QueueState, Read, Reference, ReferenceAsk, ReferenceLabel, ReferenceRead,
    Review, Summary,
};

use super::keeps_its_form;

fn number(number: u64) -> IssueNumber {
    IssueNumber::try_from(number).unwrap()
}

#[test]
fn reading_references_keeps_its_form() {
    let read = ReferencesRead {
        workspace: "w".into(),
        references: vec![
            ReferenceAsk { repository: GithubRepositoryName::named("example/project"), number: number(1021) },
            ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: number(41) },
        ],
    };

    keeps_its_form(
        &ClientRequest::ReferencesRead(read),
        r#"{"method":"references_read","workspace":"w","references":[{"repository":"example/project","number":1021},{"repository":"o/r","number":41}]}"#,
    );
}

/// An issue, and a pull request read for a chip, with its summary alone.
fn found() -> (Reference, Reference) {
    let about = About {
        url: "https://github.com/o/r/issues/1021".into(),
        title: "Decide".into(),
        author: Some("maya".into()),
        opened_at_ms: 1,
        comments: 6,
        labels: vec![ReferenceLabel { name: "needs/owner".into(), color: "fbca04".into() }],
        body: "A row".into(),
    };
    let issue = Issue {
        repository: GithubRepositoryName::named("o/r"),
        number: number(1021),
        summary: Read { said: IssueSummary { about: about.clone(), state: IssueState::NotPlanned }, read_at_ms: 2 },
    };
    let summary = Summary {
        about: About {
            url: "https://github.com/o/r/pull/1158".into(),
            author: None,
            labels: Vec::new(),
            body: String::new(),
            ..about
        },
        state: PrState::Open,
        is_draft: false,
        review: Review::Approved,
        queue: Some(QueueState::AwaitingChecks),
        merge_state: MergeState::Clean,
        mergeable: Mergeability::Mergeable,
        base_branch: "main".into(),
        head: "h".into(),
        base: "b".into(),
        added: 12,
        removed: 3,
        rollup: Some(ChecksRollup::Running),
    };
    let pull_request = PullRequest {
        repository: GithubRepositoryName::named("o/r"),
        number: PullRequestNumber::try_from(1158).unwrap(),
        summary: Read { said: summary, read_at_ms: 2 },
        history: None,
        stack: None,
        checks: None,
    };
    (Reference::Issue(issue), Reference::PullRequest(pull_request))
}

#[test]
fn what_the_hub_found_keeps_its_form() {
    let (issue, pull_request) = found();
    let reply = References {
        references: vec![
            ReferenceRead::Found { reference: Box::new(issue) },
            ReferenceRead::Found { reference: Box::new(pull_request) },
            ReferenceRead::Missing,
            ReferenceRead::Unread { message: "limited".into() },
        ],
    };

    keeps_its_form(
        &ClientResponse::References(reply),
        concat!(
            r#"{"result":"references","references":["#,
            r#"{"found":"found","reference":{"type":"issue","repository":"o/r","number":1021,"summary":{"said":{"#,
            r#""about":{"url":"https://github.com/o/r/issues/1021","title":"Decide","author":"maya","opened_at_ms":1,"#,
            r#""comments":6,"labels":[{"name":"needs/owner","color":"fbca04"}],"body":"A row"},"#,
            r#""state":"not_planned"},"read_at_ms":2}}},"#,
            r#"{"found":"found","reference":{"type":"pull_request","repository":"o/r","number":1158,"summary":{"said":{"#,
            r#""about":{"url":"https://github.com/o/r/pull/1158","title":"Decide","opened_at_ms":1,"comments":6,"#,
            r#""labels":[],"body":""},"state":"OPEN","is_draft":false,"review":"APPROVED","queue":"AWAITING_CHECKS","#,
            r#""merge_state":"CLEAN","mergeable":"mergeable","base_branch":"main","head":"h","base":"b","added":12,"#,
            r#""removed":3,"rollup":"running"},"read_at_ms":2}}},"#,
            r#"{"found":"missing"},{"found":"unread","message":"limited"}]}"#
        ),
    );
}

/// What a newer hub says that this version doesn't know reads as unknown, rather than failing the whole reply.
#[test]
fn a_newer_hubs_words_read_as_unknown() {
    let (issue, _) = found();
    let mut newer = serde_json::to_value(&issue).unwrap();
    newer["summary"]["said"]["state"] = "locked".into();
    let reply = serde_json::json!({"result": "references", "references": [
        {"found": "later"},
        {"found": "found", "reference": {"type": "discussion", "locked": true}},
        {"found": "found", "reference": newer},
    ]});

    let reply: ClientResponse = protocol::frame::decode(reply.to_string().as_bytes()).unwrap();

    let ClientResponse::References(References { references }) = reply else { panic!("{reply:?}") };
    assert!(matches!(references[0], ReferenceRead::Unknown));
    let ReferenceRead::Found { reference } = &references[1] else { panic!("{references:?}") };
    assert_eq!(**reference, Reference::Unknown);
    let ReferenceRead::Found { reference } = &references[2] else { panic!("{references:?}") };
    let Reference::Issue(issue) = &**reference else { panic!("{reference:?}") };
    assert_eq!(issue.summary.said.state, IssueState::Unknown);
}
