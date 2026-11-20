//! Settling a workspace's [`WorkspacePlacement`] at creation and driving setup, launches, terminals, archiving and
//! restoring from that recorded placement without re-querying the provider.

use protocol::capability::settings::Settings;
use protocol::client::terminals::TerminalOpen;
use protocol::client::{ErrorCode, HubNotice};
use protocol::fixtures;
use protocol::model::{AgentKind, Repo, RepoOrigin, RepoUrl, Rev, WorkspaceStatus};
use protocol::prompt::Prompt;
use protocol::provider::{ClonePersistence, WorkerEnvironment, WorkerFilesystem};
use protocol::terminal::TerminalSize;
use protocol::worker::{
    AgentSpec, ArchiveScript, NamesGitHas, PrepareWorkspace, RemoveWorkspace, RepoSource, RestoreWorkspace,
    SetupWorkspace, TerminalStart, WorkerRequest,
};

use crate::hub::repo_source::{resolve_placement, workspace_env_for};
use crate::hub::stand_in_workers::{TIMEOUT, a_turn, answering_first, hub_with_environment};
use crate::hub::tests::described;

fn remote_url() -> RepoUrl {
    RepoUrl::try_from("https://example.invalid/org/repo.git".to_owned()).unwrap()
}

fn env_var<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
    env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn assert_separate_env(env: &[(String, String)], what: &str) {
    assert_eq!(env_var(env, "CONDUCTOR_IS_LOCAL"), Some("0"), "{what}: CONDUCTOR_IS_LOCAL");
    assert_eq!(env_var(env, "CONDUCTOR_ROOT_PATH"), None, "{what}: CONDUCTOR_ROOT_PATH omitted without a hub checkout");
}

#[tokio::test]
async fn a_separate_workspace_uses_its_recorded_placement_without_re_querying_the_provider() {
    let dir = tempfile::tempdir().unwrap();
    let separate = WorkerEnvironment { filesystem: WorkerFilesystem::Separate, clones: ClonePersistence::Persistent };
    let (hub, asked) = hub_with_environment(dir.path(), separate, vec![described()], |_| None);
    let url = remote_url();
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&url)).unwrap();

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    // From here on, nothing after creation may contact the provider for its environment.
    asked.fail_environment_reads();
    let expected = RepoSource::Clone { url: url.as_str().to_owned() };

    let names = asked.requests(|r| {
        if let WorkerRequest::NamesGitHas(NamesGitHas { source, .. }) = r {
            Some(source.clone())
        } else if let WorkerRequest::PrepareWorkspace(PrepareWorkspace { source, .. }) = r {
            Some(source.clone())
        } else {
            None
        }
    });
    assert_eq!(names, [expected.clone(), expected.clone()]);

    let setup_started = || !asked.requests(|r| matches!(r, WorkerRequest::SetupWorkspace(_)).then_some(())).is_empty();
    assert!(waiting::task::up_to(TIMEOUT, setup_started).await, "setup started");
    let setup = asked.requests(|r| {
        let WorkerRequest::SetupWorkspace(SetupWorkspace { source, env, .. }) = r else { return None };
        Some((source.clone(), env.clone()))
    });
    assert_eq!(setup[0].0, expected);
    assert_separate_env(&setup[0].1, "setup");

    let chat = hub.session_create(made.id.clone(), AgentKind::Claude, Settings::default(), None).await.unwrap();
    let spec: AgentSpec = a_turn(&hub, &asked, &chat.id, Prompt::new("hi"), None).await;
    assert_separate_env(&spec.env, "agent launch");

    let open =
        TerminalOpen { workspace_id: made.id.clone(), id: "t1".into(), size: TerminalSize { cols: 80, rows: 24 } };
    hub.terminal_open(open).await.unwrap();
    let term_envs = asked.requests(|r| {
        let WorkerRequest::TerminalStart(TerminalStart { env, .. }) = r else { return None };
        Some(env.clone())
    });
    assert_separate_env(&term_envs[0], "terminal");

    assert_separate_archive_and_restore(&hub, &asked, &made.id, &expected).await;
}

async fn assert_separate_archive_and_restore(
    hub: &std::sync::Arc<crate::hub::Hub>,
    asked: &std::sync::Arc<crate::hub::stand_in_workers::Asked>,
    id: &protocol::model::WorkspaceId,
    expected: &RepoSource,
) {
    hub.workspace_archive(id.clone(), false).await.unwrap();
    let archive_envs = asked.requests(|r| {
        let WorkerRequest::ArchiveScript(ArchiveScript { source, env, .. }) = r else { return None };
        Some((source.clone(), env.clone()))
    });
    assert_eq!(&archive_envs[0].0, expected);
    assert_separate_env(&archive_envs[0].1, "archive script");
    let removals = asked.requests(|r| {
        let WorkerRequest::RemoveWorkspace(RemoveWorkspace { source, .. }) = r else { return None };
        Some(source.clone())
    });
    assert_eq!(removals, std::slice::from_ref(expected));

    answering_first(hub, id, asked, |_| None);
    let back = hub.workspace_restore(id.clone()).await.unwrap();
    assert_eq!(back.status, WorkspaceStatus::Ready);
    let restore_calls = asked.requests(|r| {
        let WorkerRequest::RestoreWorkspace(RestoreWorkspace { source, .. }) = r else { return None };
        Some(source.clone())
    });
    assert_eq!(restore_calls, std::slice::from_ref(expected));
}

#[tokio::test]
async fn creation_refuses_unusable_environments_before_recording_a_workspace_or_creating_an_instance() {
    let cases = [
        ("separate without remote address", WorkerEnvironment::default()),
        (
            "unknown filesystem",
            WorkerEnvironment { filesystem: WorkerFilesystem::Unknown, clones: ClonePersistence::Persistent },
        ),
        (
            "unknown clone persistence",
            WorkerEnvironment { filesystem: WorkerFilesystem::Hub, clones: ClonePersistence::Unknown },
        ),
    ];
    for (case, env) in cases {
        let dir = tempfile::tempdir().unwrap();
        let (hub, asked) = hub_with_environment(dir.path(), env, vec![described()], |_| None);
        let mut feed = hub.subscribe_feed();

        let err = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap_err();

        assert_eq!(err.code, ErrorCode::Unavailable, "{case}");
        assert_eq!(asked.instances_created(), 0, "{case}: must not ask provider to create an instance");
        let notices: Vec<_> = std::iter::from_fn(|| feed.try_recv().ok())
            .filter(|notice| matches!(notice, HubNotice::Workspace(_) | HubNotice::Removed { .. }))
            .collect();
        assert!(notices.is_empty(), "{case}: must not record or announce a workspace: {notices:?}");
    }
}

#[test]
fn hub_machine_uses_clone_source_for_remote_only_repo_and_omits_root_path() {
    let url = remote_url();
    let repo = Repo {
        id: "r".into(),
        name: "repo".into(),
        origin: RepoOrigin::Remote { url: url.clone() },
        created_at_ms: 0,
        rev: Rev::default(),
    };
    let placement = resolve_placement(&repo, "local", WorkerEnvironment::HUB_MACHINE).unwrap();
    assert_eq!(placement.source, RepoSource::Clone { url: url.as_str().to_owned() });

    let env = workspace_env_for(&fixtures::workspace("w"), &placement);
    assert_eq!(env_var(&env, "CONDUCTOR_IS_LOCAL"), Some("1"));
    assert_eq!(env_var(&env, "CONDUCTOR_ROOT_PATH"), None);
}
