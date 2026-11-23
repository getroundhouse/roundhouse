use super::{
    AccountIdentity, Observation, ObservationKey, Observed, OwnPullRequestDetail, OwnPullRequestSaid, ReadFrom,
    ReferenceSaid,
};
use crate::model::{ChangeSummary, GithubRepositoryName, IssueNumber, Level, LineCounts, ReferenceAsk, Rev};

/// The level an observation is sent at, and which subscriptions are sent it.
type Sent = (Option<Level>, Option<ReadFrom>);

/// An observation of each kind, with the key and revision it's of, and how it's sent.
fn of_each_kind() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let unknown = (Observation::Unknown, ObservationKey::Unknown, Rev::default(), (None, None));
    [of_a_workspace(), of_github(), of_an_account(), vec![unknown]].concat()
}

/// Of each kind of what git says of a workspace.
fn of_a_workspace() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let lines = Observed::Current { value: LineCounts { added: 3, removed: 1 } };
    let changes = Observed::AsOf { value: ChangeSummary::new("c1".into(), Vec::new()), as_of_ms: 1_000 };
    vec![
        (
            Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(4) },
            ObservationKey::WorkspaceLines { workspace_id: "w".into() },
            Rev::new(4),
            (Some(Level::Hub), Some(ReadFrom::Every)),
        ),
        (
            Observation::WorkspaceChanges { workspace_id: "w".into(), changes, rev: Rev::new(5) },
            ObservationKey::WorkspaceChanges { workspace_id: "w".into() },
            Rev::new(5),
            (Some(Level::Workspace { id: "w".into() }), Some(ReadFrom::Every)),
        ),
    ]
}

/// Of each kind of what GitHub says.
fn of_github() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let reference =
        ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(41).unwrap() };
    vec![
        (
            Observation::Reference {
                reference: reference.clone(),
                said: Observed::AsOf { value: ReferenceSaid::Missing, as_of_ms: 1_000 },
                rev: Rev::new(6),
            },
            ObservationKey::Reference { reference: reference.clone() },
            Rev::new(6),
            (Some(Level::Reference { reference }), Some(ReadFrom::GithubObserved)),
        ),
        (
            Observation::WorkspacePullRequest {
                workspace_id: "w".into(),
                pull_request: Observed::AsOf { value: OwnPullRequestSaid { pull_request: None }, as_of_ms: 1_000 },
                rev: Rev::new(7),
            },
            ObservationKey::WorkspacePullRequest { workspace_id: "w".into() },
            Rev::new(7),
            (Some(Level::Hub), Some(ReadFrom::GithubObserved)),
        ),
        (
            Observation::WorkspacePullRequestDetail {
                workspace_id: "w".into(),
                detail: Observed::Current { value: OwnPullRequestDetail { pull_request: None } },
                rev: Rev::new(8),
            },
            ObservationKey::WorkspacePullRequestDetail { workspace_id: "w".into() },
            Rev::new(8),
            (Some(Level::Workspace { id: "w".into() }), Some(ReadFrom::GithubObserved)),
        ),
        (
            Observation::WorkspaceRepositoryUnseen {
                workspace_id: "w".into(),
                unseen: Observed::Current { value: None },
                rev: Rev::new(9),
            },
            ObservationKey::WorkspaceRepositoryUnseen { workspace_id: "w".into() },
            Rev::new(9),
            (Some(Level::Workspace { id: "w".into() }), Some(ReadFrom::GithubObserved)),
        ),
    ]
}

/// Of each kind of what an agent's account is, as its agents say.
fn of_an_account() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    vec![
        (
            Observation::AccountIdentity {
                account: "a".into(),
                identity: Observed::AsOf { value: AccountIdentity::default(), as_of_ms: 1_000 },
                rev: Rev::new(10),
            },
            ObservationKey::AccountIdentity { account: "a".into() },
            Rev::new(10),
            (Some(Level::Hub), Some(ReadFrom::EveryObservation)),
        ),
        (
            Observation::AccountLimits { account: "a".into(), limits: Observed::NotKnown, rev: Rev::new(11) },
            ObservationKey::AccountLimits { account: "a".into() },
            Rev::new(11),
            (Some(Level::Hub), Some(ReadFrom::EveryObservation)),
        ),
    ]
}

#[test]
fn an_observation_says_which_it_is_its_revision_the_level_it_is_sent_at_and_who_reads_it() {
    for (observation, key, rev, sent) in of_each_kind() {
        assert_eq!((observation.key(), observation.rev()), (key.clone(), rev), "{observation:?}");
        assert_eq!((key.level(), key.read_from()), sent, "{key:?}");
    }
}
