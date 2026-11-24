use super::{
    AccountIdentity, ConversationCache, GithubAppSaid, Observation, ObservationKey, Observed, OtherBuild,
    OwnPullRequestDetail, OwnPullRequestSaid, ReadFrom, ReferenceSaid, WaitRead,
};
use crate::capability::output::TodoList;
use crate::capability::settings::ModelCatalog;
use crate::model::{
    AgentKind, BranchName, Branches, ChangeSummary, ContextUsage, GithubRepositoryName, Head, IssueNumber, Level,
    LineCounts, ModelListKey, ReferenceAsk, Rev,
};

/// The level an observation is sent at, and which subscriptions are sent it.
type Sent = (Option<Level>, Option<ReadFrom>);

/// An observation of each kind, with the key and revision it's of, and how it's sent.
fn of_each_kind() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let unknown = (Observation::Unknown, ObservationKey::Unknown, Rev::default(), (None, None));
    [
        of_a_workspace(),
        of_github(),
        of_a_workspace_worker(),
        of_a_repository(),
        of_a_github_account(),
        of_an_app(),
        of_an_agent(),
        of_an_account(),
        of_a_session(),
        vec![unknown],
    ]
    .concat()
}

/// Of each kind of what an agent says of itself.
fn of_an_agent() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let list = ModelListKey {
        agent: AgentKind::Codex,
        repo: "r".into(),
        provider: "local".into(),
        account: None,
        cli_version: None,
    };
    let catalog = Observed::AsOf { value: ModelCatalog::default(), as_of_ms: 1_000 };
    vec![(
        Observation::ModelCatalog { list: list.clone(), catalog, rev: Rev::new(12) },
        ObservationKey::ModelCatalog { list },
        Rev::new(12),
        (Some(Level::Hub), Some(ReadFrom::EveryObservation)),
    )]
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

/// Of each kind of what a workspace's worker says of its worktree, and of itself.
fn of_a_workspace_worker() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let branches = Branches { head: Head::Branch { name: BranchName::from("aspen") }, had: Vec::new() };
    let build = Observed::Current { value: OtherBuild { said: Some("0.9.0".into()) } };
    vec![
        (
            Observation::WorkspaceBranches {
                workspace_id: "w".into(),
                branches: Observed::Current { value: branches },
                rev: Rev::new(14),
            },
            ObservationKey::WorkspaceBranches { workspace_id: "w".into() },
            Rev::new(14),
            (Some(Level::Workspace { id: "w".into() }), Some(ReadFrom::EveryObservation)),
        ),
        (
            Observation::WorkspaceWorkerBuild { workspace_id: "w".into(), build, rev: Rev::new(15) },
            ObservationKey::WorkspaceWorkerBuild { workspace_id: "w".into() },
            Rev::new(15),
            (Some(Level::Hub), Some(ReadFrom::EveryObservation)),
        ),
    ]
}

/// Of each kind of what GitHub says of a repository.
fn of_a_repository() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    vec![(
        Observation::RepositoryReach {
            repo: "r".into(),
            reach: Observed::Current { value: crate::model::Reach::NotOnGithub },
            rev: Rev::new(13),
        },
        ObservationKey::RepositoryReach { repo: "r".into() },
        Rev::new(13),
        (Some(Level::Hub), Some(ReadFrom::EveryObservation)),
    )]
}

/// Of each kind of what GitHub says of a GitHub account the hub holds a login for.
fn of_a_github_account() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    vec![(
        Observation::GithubUsername {
            account: "g".into(),
            username: Observed::Current { value: "ada".into() },
            rev: Rev::new(12),
        },
        ObservationKey::GithubUsername { account: "g".into() },
        Rev::new(12),
        (Some(Level::Hub), Some(ReadFrom::EveryObservation)),
    )]
}

/// Of each kind of what GitHub says of an app of your own, and of where each of its installations is.
fn of_an_app() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    let app = crate::model::GithubAppId::new(9);
    let bot = crate::model::GithubUser { id: crate::model::GithubUserId::new(41), name: "ada-roundhouse[bot]".into() };
    let said = GithubAppSaid { slug: "ada-roundhouse".into(), name: "Ada's".into(), owner: bot.clone() };
    let at_the_hub = (Some(Level::Hub), Some(ReadFrom::EveryObservation));
    vec![
        (
            Observation::GithubAppSaid { app, said: Observed::Current { value: said }, rev: Rev::new(14) },
            ObservationKey::GithubAppSaid { app },
            Rev::new(14),
            at_the_hub.clone(),
        ),
        (
            Observation::GithubAppBot { app, bot: Observed::Current { value: bot }, rev: Rev::new(15) },
            ObservationKey::GithubAppBot { app },
            Rev::new(15),
            at_the_hub.clone(),
        ),
        (
            Observation::InstalledOn {
                account: "g".into(),
                on: Observed::Current { value: "adalabs".into() },
                rev: Rev::new(16),
            },
            ObservationKey::InstalledOn { account: "g".into() },
            Rev::new(16),
            at_the_hub,
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

/// Of each kind of what an agent says of its chat.
fn of_a_session() -> Vec<(Observation, ObservationKey, Rev, Sent)> {
    vec![session_context(), session_todos(), session_cache(), wait_read()]
}

/// How full a session's context is, with its key and revision, and how it's sent.
fn session_context() -> (Observation, ObservationKey, Rev, Sent) {
    let context =
        ContextUsage { used_tokens: 1, max_tokens: 2, model_tokens: None, max_set_by: None, parts: Vec::new() };
    (
        Observation::SessionContext {
            session_id: "s".into(),
            context: Observed::AsOf { value: context, as_of_ms: 1_000 },
            rev: Rev::new(21),
        },
        ObservationKey::SessionContext { session_id: "s".into() },
        Rev::new(21),
        (Some(Level::Session { id: "s".into() }), Some(ReadFrom::EveryObservation)),
    )
}

/// A session's to-do list, with its key and revision, and how it's sent.
fn session_todos() -> (Observation, ObservationKey, Rev, Sent) {
    (
        Observation::SessionTodos {
            session_id: "s".into(),
            todos: Observed::AsOf { value: TodoList::default(), as_of_ms: 1_000 },
            rev: Rev::new(12),
        },
        ObservationKey::SessionTodos { session_id: "s".into() },
        Rev::new(12),
        (Some(Level::Session { id: "s".into() }), Some(ReadFrom::EveryObservation)),
    )
}

/// What the API's cache holds of a session's conversation, with its key and revision, and how it's sent.
fn session_cache() -> (Observation, ObservationKey, Rev, Sent) {
    let cache = ConversationCache { held: None, worth_telling: true };
    (
        Observation::SessionCache {
            session_id: "s".into(),
            cache: Observed::AsOf { value: cache, as_of_ms: 1_000 },
            rev: Rev::new(13),
        },
        ObservationKey::SessionCache { session_id: "s".into() },
        Rev::new(13),
        (Some(Level::Hub), Some(ReadFrom::EveryObservation)),
    )
}

/// What a wait last read, with its key and revision, and how it's sent.
fn wait_read() -> (Observation, ObservationKey, Rev, Sent) {
    let read = WaitRead {
        pull_requests: Vec::new(),
        observed_at_ms: 1,
        read_error: None,
        last_attempt_at_ms: 1,
        affected: Vec::new(),
    };
    (
        Observation::WaitRead {
            session_id: "s".into(),
            wait: "a".into(),
            read: Observed::AsOf { value: read, as_of_ms: 1 },
            rev: Rev::new(14),
        },
        ObservationKey::WaitRead { session_id: "s".into(), wait: "a".into() },
        Rev::new(14),
        (Some(Level::Session { id: "s".into() }), Some(ReadFrom::EveryObservation)),
    )
}

/// A wait's watch is what its record keeps with what it last read, whatever it read.
#[test]
fn a_watch_is_its_record_and_its_read() {
    let mut watch = *crate::fixtures::pull_request_wait("a").watch.unwrap();
    watch.read_error = Some("offline".into());
    watch.last_attempt_at_ms = 9;
    watch.affected = vec![crate::model::PullRequestNumber::try_from(7).unwrap()];

    let recorded = watch.recorded();

    assert_eq!((recorded.snapshot.pull_requests.len(), &recorded.read_error, recorded.affected.len()), (0, &None, 0));
    assert_eq!((&recorded.requested, &recorded.snapshot.repository), (&watch.requested, &watch.snapshot.repository));
    assert_eq!(recorded.with_read(watch.read()), watch);
}

#[test]
fn an_observation_says_which_it_is_its_revision_the_level_it_is_sent_at_and_who_reads_it() {
    for (observation, key, rev, sent) in of_each_kind() {
        assert_eq!((observation.key(), observation.rev()), (key.clone(), rev), "{observation:?}");
        assert_eq!((key.level(), key.read_from()), sent, "{key:?}");
    }
}
