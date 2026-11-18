//! What the hub does with its agents' descriptions, against a hub that never starts a worker: what a new chat runs,
//! what a chat is refused, and whether the agent is asked to compact.

use crate::session::ChatSettings;
use protocol::capability::settings::Effort;
use protocol::capability::settings::Settings;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AgentDescription, AgentKind, ModelName, WorkspaceId};

use crate::hub::tests::{claimed, described, hub_describing, hub_in};

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

    let made = hub.session_create(WorkspaceId::from(W), AgentKind::Claude, Settings::default(), None);
    let made = made.await.unwrap();
    let chosen = Settings { effort: Some(Effort::Low), ..Settings::default() };
    let low = hub.session_create(W.into(), AgentKind::Claude, chosen, None).await.unwrap();

    let defaults = described().defaults;
    let (model, effort) = (defaults.model.unwrap(), defaults.effort.unwrap());
    let given = ChatSettings { model, mode: protocol::fixtures::full_access(), effort };
    assert_eq!(ChatSettings::of(&made), given, "its kind's defaults, in the mode its claims give first");
    let sonnet = ModelName::named("sonnet");
    let expected = ChatSettings { model: sonnet, mode: protocol::fixtures::full_access(), effort: Effort::Low };
    assert_eq!(ChatSettings::of(&low), expected);
}

#[tokio::test]
async fn a_kind_of_agent_the_hub_has_no_description_of_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_describing(dir.path(), Vec::new());
    ready(&hub);

    let other = tempfile::tempdir().unwrap();
    let claudes = hub_in(other.path());
    ready(&claudes);

    let refused = hub.session_create(W.into(), AgentKind::Claude, Settings::default(), None).await;
    let newer = claudes.session_create(W.into(), AgentKind::Unknown, Settings::default(), None).await;

    assert_eq!(code(&refused), Some(ErrorCode::InvalidRequest), "{refused:?}");
    assert_eq!(hub.session_list(W.into()).await.unwrap().len(), 1, "no chat was made");
    assert_eq!(code(&newer), Some(ErrorCode::InvalidRequest), "a kind from a newer client: {newer:?}");
    assert_eq!(claudes.session_list(W.into()).await.unwrap().len(), 1, "no chat of a kind from a newer client");
}

/// One this hub doesn't know, as from a newer client, as well as one the agent's description leaves out.
#[tokio::test]
async fn an_effort_or_a_mode_the_agent_isnt_offered_is_refused_for_a_new_chat_and_for_a_change() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    ready(&hub);
    let at = |effort| Settings { effort: Some(effort), ..Settings::default() };
    let in_mode = |mode| Settings { mode: Some(mode), ..Settings::default() };
    let refused = [
        ("an effort it isn't offered", at(Effort::Max)),
        ("an effort this hub doesn't know", at(Effort::Unknown)),
        ("a mode this hub doesn't know", in_mode(protocol::capability::approvals::ApprovalMode::UNKNOWN)),
    ];
    let before = hub.session_get("s".into()).await.unwrap();

    for (case, change) in refused {
        let new_chat = hub.session_create(W.into(), AgentKind::Claude, change.clone(), None).await;
        let changed = hub.session_configure("s".into(), change).await;

        assert_eq!(code(&new_chat), Some(ErrorCode::InvalidRequest), "{case}, for a new chat: {new_chat:?}");
        assert_eq!(code(&changed), Some(ErrorCode::InvalidRequest), "{case}, for a change: {changed:?}");
        assert_eq!(hub.session_list(W.into()).await.unwrap().len(), 1, "{case}: no chat was made");
        let kept = hub.session_get("s".into()).await.unwrap();
        assert_eq!(ChatSettings::of(&kept), ChatSettings::of(&before), "{case}: the chat's settings didn't change");
    }
    // What the agent is offered is taken, each leaving the rest as it was.
    let low = ChatSettings { effort: Effort::Low, ..ChatSettings::of(&before) };
    for (case, taken, expected) in [
        ("an effort it's offered", at(Effort::Low), low.clone()),
        (
            "plan mode",
            in_mode(protocol::fixtures::plan_mode()),
            ChatSettings { mode: protocol::fixtures::plan_mode(), ..low.clone() },
        ),
        (
            "full access",
            in_mode(protocol::fixtures::full_access()),
            ChatSettings { mode: protocol::fixtures::full_access(), ..low.clone() },
        ),
    ] {
        let changed = hub.session_configure("s".into(), taken).await;

        let changed = changed.unwrap_or_else(|e| panic!("{case} was refused: {e:?}"));
        assert_eq!(ChatSettings::of(&changed), expected, "{case}");
    }
}

/// Which efforts a model takes is its own, so a change is judged by the settings it leaves the chat with, not by what
/// it names alone: an effort the agent is offered, but not with the chat's model, and a model that isn't offered the
/// chat's effort, are each refused, though neither change names both.
#[tokio::test]
async fn a_change_is_refused_for_the_settings_it_leaves_the_chat_with_as_well_as_for_what_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut claims = claimed();
    let only_low = serde_json::json!({ "model": "haiku", "label": "Haiku", "efforts": ["low"] });
    let offered = vec![serde_json::from_value(only_low).unwrap()];
    claims.settings.models = Some(protocol::capability::settings::Models::Static { offered });
    let hub = hub_describing(dir.path(), vec![AgentDescription { capabilities: Some(claims), ..described() }]);
    ready(&hub);
    let (haiku, sonnet) = (ModelName::named("haiku"), ModelName::named("sonnet"));
    let on = |model: &ModelName, effort| Settings {
        model: Some(model.clone()),
        effort: Some(effort),
        ..Settings::default()
    };
    let at_low_on_haiku = hub.session_create(W.into(), AgentKind::Claude, on(&haiku, Effort::Low), None).await.unwrap();
    let at_high_on_sonnet = hub.session_configure("s".into(), on(&sonnet, Effort::High)).await.unwrap();

    let high = Settings { effort: Some(Effort::High), ..Settings::default() };
    let to_haiku = Settings { model: Some(haiku), ..Settings::default() };
    for (case, chat, change) in [
        ("an effort the chat's model isn't offered", &at_low_on_haiku, high),
        ("a model the chat's effort isn't offered for", &at_high_on_sonnet, to_haiku),
    ] {
        let changed = hub.session_configure(chat.id.clone(), change).await;

        assert_eq!(code(&changed), Some(ErrorCode::InvalidRequest), "{case}: {changed:?}");
        let kept = hub.session_get(chat.id.clone()).await.unwrap();
        assert_eq!(ChatSettings::of(&kept), ChatSettings::of(chat), "{case}: the chat's settings didn't change");
    }
}

#[tokio::test]
async fn only_an_agent_that_compacts_when_asked_is_asked_to() {
    let dir = tempfile::tempdir().unwrap();
    let mut claims = claimed();
    claims.context.compaction = None;
    let doesnt = AgentDescription { features: Vec::new(), capabilities: Some(claims), ..described() };
    let doesnt = hub_describing(dir.path(), vec![doesnt]);
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
