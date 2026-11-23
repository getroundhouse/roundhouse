//! A workspace's pull request as observations, spelled out, since a client and its hub can each be a different build:
//! how it stands, at the hub's level, its checks, history and stack, and whether GitHub showed its repository, at the
//! workspace's.

use protocol::client::HubNotice;
use protocol::model::{
    Checks, Local, Observation, ObservationKey, Observed, OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary,
    PullRequestDetail, PullRequestSaid, Rev,
};

use crate::keeps_its_form;

#[test]
fn a_workspace_s_pull_request_keeps_its_form_where_its_branch_has_none() {
    let none = OwnPullRequestSaid { pull_request: None };

    keeps_its_form(
        &HubNotice::Observation(Observation::WorkspacePullRequest {
            workspace_id: "w".into(),
            pull_request: Observed::AsOf { value: none, as_of_ms: 1_000 },
            rev: Rev::new(5),
        }),
        concat!(
            r#"{"notice":"observation","kind":"workspace_pull_request","workspace_id":"w","#,
            r#""pull_request":{"freshness":"as_of","value":{},"as_of_ms":1000},"rev":5}"#
        ),
    );
    keeps_its_form(
        &HubNotice::ObservationRemoved {
            observation: ObservationKey::WorkspacePullRequest { workspace_id: "w".into() },
            rev: Rev::new(6),
        },
        r#"{"notice":"observation_removed","observation":{"kind":"workspace_pull_request","workspace_id":"w"},"rev":6}"#,
    );
}

#[test]
fn a_workspace_s_pull_request_s_detail_keeps_its_form() {
    let detail = PullRequestDetail {
        repository: protocol::model::GithubRepositoryName::named("o/r"),
        number: protocol::model::PullRequestNumber::try_from(7).unwrap(),
        head: "head-7".into(),
        checks: Some(Checks { revision: None, requirements_known: true, checks: Vec::new() }),
        history: None,
        stack: None,
    };

    keeps_its_form(
        &HubNotice::Observation(Observation::WorkspacePullRequestDetail {
            workspace_id: "w".into(),
            detail: Observed::Current { value: OwnPullRequestDetail { pull_request: Some(detail) } },
            rev: Rev::new(5),
        }),
        concat!(
            r#"{"notice":"observation","kind":"workspace_pull_request_detail","workspace_id":"w","detail":"#,
            r#"{"freshness":"current","value":{"pull_request":{"repository":"o/r","number":7,"head":"head-7","#,
            r#""checks":{"revision":null,"requirements_known":true,"checks":[]}}}},"rev":5}"#
        ),
    );
    keeps_its_form(
        &HubNotice::Observation(Observation::WorkspaceRepositoryUnseen {
            workspace_id: "w".into(),
            unseen: Observed::Current { value: None },
            rev: Rev::new(6),
        }),
        concat!(
            r#"{"notice":"observation","kind":"workspace_repository_unseen","workspace_id":"w","#,
            r#""unseen":{"freshness":"current","value":null},"rev":6}"#
        ),
    );
}

/// A pull request found reads back as it was written, its summary and how the worktree stands against it whole.
#[test]
fn a_workspace_s_pull_request_found_reads_back_whole() {
    let pull_request = protocol::fixtures::pull_request(7);
    let said = PullRequestSaid {
        repository: pull_request.repository,
        number: pull_request.number,
        summary: pull_request.summary.said,
    };
    let local = Local { uncommitted: 2, ahead: 1, behind: 0 };
    let observation = Observation::WorkspacePullRequest {
        workspace_id: "w".into(),
        pull_request: Observed::Current {
            value: OwnPullRequestSaid { pull_request: Some(OwnSummary { pull_request: said, local }) },
        },
        rev: Rev::new(5),
    };

    let line = serde_json::to_string(&observation).unwrap();

    assert_eq!(serde_json::from_str::<Observation>(&line).unwrap(), observation, "{line}");
}
