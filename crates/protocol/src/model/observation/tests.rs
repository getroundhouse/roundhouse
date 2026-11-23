use super::{Observation, ObservationKey, Observed, OwnPullRequestDetail, OwnPullRequestSaid, ReferenceSaid};
use crate::model::{ChangeSummary, GithubRepositoryName, IssueNumber, Level, LineCounts, ReferenceAsk, Rev};

/// The level an observation is sent at, and whether it's of what GitHub says.
type Sent = (Option<Level>, bool);

/// An observation of each kind, with the key and revision it's of, and how it's sent.
fn of_each_kind() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let lines = Observed::Current { value: LineCounts { added: 3, removed: 1 } };
    let changes = Observed::AsOf { value: ChangeSummary::new("c1".into(), Vec::new()), as_of_ms: 1_000 };
    let reference =
        ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(41).unwrap() };
    vec![
        (
            Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(4) },
            ObservationKey::WorkspaceLines { workspace_id: "w".into() },
            Rev::new(4),
            (Some(Level::Hub), false),
        ),
        (
            Observation::WorkspaceChanges { workspace_id: "w".into(), changes, rev: Rev::new(5) },
            ObservationKey::WorkspaceChanges { workspace_id: "w".into() },
            Rev::new(5),
            (Some(Level::Workspace { id: "w".into() }), false),
        ),
        (
            Observation::Reference {
                reference: reference.clone(),
                said: Observed::AsOf { value: ReferenceSaid::Missing, as_of_ms: 1_000 },
                rev: Rev::new(6),
            },
            ObservationKey::Reference { reference: reference.clone() },
            Rev::new(6),
            (Some(Level::Reference { reference }), true),
        ),
        (
            Observation::WorkspacePullRequest {
                workspace_id: "w".into(),
                pull_request: Observed::AsOf { value: OwnPullRequestSaid { pull_request: None }, as_of_ms: 1_000 },
                rev: Rev::new(7),
            },
            ObservationKey::WorkspacePullRequest { workspace_id: "w".into() },
            Rev::new(7),
            (Some(Level::Hub), true),
        ),
        (
            Observation::WorkspacePullRequestDetail {
                workspace_id: "w".into(),
                detail: Observed::Current { value: OwnPullRequestDetail { pull_request: None } },
                rev: Rev::new(8),
            },
            ObservationKey::WorkspacePullRequestDetail { workspace_id: "w".into() },
            Rev::new(8),
            (Some(Level::Workspace { id: "w".into() }), true),
        ),
        (
            Observation::WorkspaceRepositoryUnseen {
                workspace_id: "w".into(),
                unseen: Observed::Current { value: None },
                rev: Rev::new(9),
            },
            ObservationKey::WorkspaceRepositoryUnseen { workspace_id: "w".into() },
            Rev::new(9),
            (Some(Level::Workspace { id: "w".into() }), true),
        ),
        (Observation::Unknown, ObservationKey::Unknown, Rev::default(), (None, false)),
    ]
}

#[test]
fn an_observation_says_which_it_is_its_revision_the_level_it_is_sent_at_and_whether_it_is_github_s() {
    for (observation, key, rev, sent) in of_each_kind() {
        assert_eq!((observation.key(), observation.rev()), (key.clone(), rev), "{observation:?}");
        assert_eq!((key.level(), key.is_github()), sent, "{key:?}");
    }
}
