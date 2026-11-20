//! A client's read of the issues and pull requests a chat links to, and the hub's answer, which every peer from now on
//! reads as they were first written.

use protocol::client::references::{References, ReferencesRead};
use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{
    ChecksRollup, GithubRepositoryName, IssueNumber, IssueState, PullRequestState, Reference, ReferenceAsk,
    ReferenceKind, ReferenceLabel, ReferenceRead, ReferencedPullRequest, Review,
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

#[test]
fn what_the_hub_found_keeps_its_form() {
    let issue = Reference {
        repository: GithubRepositoryName::named("o/r"),
        number: number(1021),
        kind: ReferenceKind::Issue { state: IssueState::NotPlanned },
        url: "https://github.com/o/r/issues/1021".into(),
        title: "Decide".into(),
        author: Some("maya".into()),
        opened_at_ms: 1,
        comments: 6,
        labels: vec![ReferenceLabel { name: "needs/owner".into(), color: "fbca04".into() }],
        body: "A row".into(),
    };
    let pull_request = Reference {
        kind: ReferenceKind::PullRequest(ReferencedPullRequest {
            state: PullRequestState::Draft,
            added: 12,
            removed: 3,
            checks: Some(ChecksRollup::Running),
            review: Review::Approved,
        }),
        number: number(1158),
        url: "https://github.com/o/r/pull/1158".into(),
        author: None,
        labels: Vec::new(),
        body: String::new(),
        ..issue.clone()
    };
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
            r#"{"found":"found","reference":{"repository":"o/r","number":1021,"#,
            r#""kind":{"type":"issue","state":"not_planned"},"#,
            r#""url":"https://github.com/o/r/issues/1021","title":"Decide","author":"maya","#,
            r#""opened_at_ms":1,"comments":6,"labels":[{"name":"needs/owner","color":"fbca04"}],"body":"A row"}},"#,
            r#"{"found":"found","reference":{"repository":"o/r","number":1158,"#,
            r#""kind":{"type":"pull_request","state":"draft","added":12,"removed":3,"checks":"running","review":"APPROVED"},"#,
            r#""url":"https://github.com/o/r/pull/1158","title":"Decide","opened_at_ms":1,"#,
            r#""comments":6,"labels":[],"body":""}},"#,
            r#"{"found":"missing"},{"found":"unread","message":"limited"}]}"#
        ),
    );
}

/// What a newer hub says that this version doesn't know reads as unknown, rather than failing the whole reply.
#[test]
fn a_newer_hubs_words_read_as_unknown() {
    let reply: ClientResponse = protocol::frame::decode(
        br#"{"result":"references","references":[{"found":"later"},{"found":"found","reference":{"repository":"o/r","number":1,"kind":{"type":"discussion","locked":true},"url":"u","title":"t","opened_at_ms":1,"comments":0}},{"found":"found","reference":{"repository":"o/r","number":2,"kind":{"type":"pull_request","state":"queued","added":0,"removed":0,"checks":"stale"},"url":"u","title":"t","opened_at_ms":1,"comments":0}},{"found":"found","reference":{"repository":"o/r","number":3,"kind":{"type":"issue","state":"locked"},"url":"u","title":"t","opened_at_ms":1,"comments":0}}]}"#,
    )
    .unwrap();
    let ClientResponse::References(References { references }) = reply else { panic!("{reply:?}") };
    let kinds: Vec<_> = references
        .iter()
        .map(|read| match read {
            ReferenceRead::Found { reference } => Some(reference.kind),
            _ => None,
        })
        .collect();
    let unknown_pull_request = ReferencedPullRequest {
        state: PullRequestState::Unknown,
        added: 0,
        removed: 0,
        checks: Some(ChecksRollup::Unknown),
        review: Review::default(),
    };
    assert_eq!(
        kinds,
        [
            None,
            Some(ReferenceKind::Unknown),
            Some(ReferenceKind::PullRequest(unknown_pull_request)),
            Some(ReferenceKind::Issue { state: IssueState::Unknown }),
        ]
    );
    assert!(matches!(references[0], ReferenceRead::Unknown));
}
