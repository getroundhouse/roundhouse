//! What the hub does with its agents' descriptions, against a hub that never starts a worker: what a new chat runs,
//! what a chat is refused, and whether the agent is asked to compact.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    AgentDescription, AgentKind, Effort, ModelName, PermissionMode, SessionSettings, SessionSettingsChange, WorkspaceId,
};

use crate::hub::tests::{described, hub_describing, hub_in};

const W: &str = "w";

fn ready(hub: &crate::hub::Hub) {
    hub.store.workspace_ready(&W.into(), "/w", &"alder".into(), "main", None).unwrap();
}

/// What a refusal's code is, or `None` for what was done.
fn code<T>(result: &Result<T, ApiError>) -> Option<ErrorCode> {
    result.as_ref().err().map(|e| e.code)
}

#[tokio::test]
async fn a_new_chat_runs_what_its_agents_description_says_unless_it_names_something_else() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    ready(&hub);

    let made = hub.session_create(WorkspaceId::from(W), AgentKind::Claude, SessionSettingsChange::default(), None);
    let made = made.await.unwrap();
    let chosen = SessionSettingsChange { effort: Some(Effort::Low), ..SessionSettingsChange::default() };
    let low = hub.session_create(W.into(), AgentKind::Claude, chosen, None).await.unwrap();

    assert_eq!(made.settings(), described().defaults);
    let sonnet = ModelName::named("sonnet");
    let expected = SessionSettings { model: sonnet, permission_mode: PermissionMode::FullAccess, effort: Effort::Low };
    assert_eq!(low.settings(), expected);
}

#[tokio::test]
async fn a_kind_of_agent_the_hub_has_no_description_of_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_describing(dir.path(), Vec::new());
    ready(&hub);

    let other = tempfile::tempdir().unwrap();
    let claudes = hub_in(other.path());
    ready(&claudes);

    let refused = hub.session_create(W.into(), AgentKind::Claude, SessionSettingsChange::default(), None).await;
    let newer = claudes.session_create(W.into(), AgentKind::Unknown, SessionSettingsChange::default(), None).await;

    assert_eq!(code(&refused), Some(ErrorCode::InvalidRequest), "{refused:?}");
    assert_eq!(hub.session_list(W.into()).await.unwrap().len(), 1, "no chat was made");
    assert_eq!(code(&newer), Some(ErrorCode::InvalidRequest), "a kind from a newer client: {newer:?}");
}

#[tokio::test]
async fn an_effort_or_a_mode_the_agent_isnt_offered_is_refused_for_a_new_chat_and_for_a_change() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    ready(&hub);
    let at_max = SessionSettingsChange { effort: Some(Effort::Max), ..SessionSettingsChange::default() };
    let unknown_mode =
        SessionSettingsChange { permission_mode: Some(PermissionMode::Unknown), ..SessionSettingsChange::default() };
    let before = hub.session_get("s".into()).await.unwrap();

    let new_chat = hub.session_create(W.into(), AgentKind::Claude, at_max.clone(), None).await;
    let change = hub.session_configure("s".into(), at_max).await;
    let mode = hub.session_configure("s".into(), unknown_mode).await;
    let offered = SessionSettingsChange { effort: Some(Effort::Low), ..SessionSettingsChange::default() };
    let taken = hub.session_configure("s".into(), offered).await.unwrap();

    for refused in [new_chat.map(|_| ()), change.map(|_| ()), mode.map(|_| ())] {
        assert_eq!(code(&refused), Some(ErrorCode::InvalidRequest), "{refused:?}");
    }
    assert_eq!(hub.session_list(W.into()).await.unwrap().len(), 1, "no chat was made");
    assert_eq!(taken.settings(), SessionSettings { effort: Effort::Low, ..before.settings() });
}

#[tokio::test]
async fn only_an_agent_that_compacts_when_asked_is_asked_to() {
    let dir = tempfile::tempdir().unwrap();
    let doesnt = hub_describing(dir.path(), vec![AgentDescription { features: Vec::new(), ..described() }]);
    ready(&doesnt);

    let refused = doesnt.session_compact("s".into(), None).await;

    assert_eq!(code(&refused), Some(ErrorCode::InvalidRequest), "{refused:?}");
    let other = tempfile::tempdir().unwrap();
    let does = hub_in(other.path());
    ready(&does);
    // Past the description, to the session's own refusal, since its agent has had no turn.
    let refused = does.session_compact("s".into(), None).await;
    assert_eq!(code(&refused), Some(ErrorCode::Conflict), "{refused:?}");
}

#[tokio::test]
async fn the_hub_lists_each_kind_it_was_given() {
    let dir = tempfile::tempdir().unwrap();

    let hub = hub_in(dir.path());

    assert_eq!(hub.agent_list(), [described()]);
}
