//! What a change of a chat's model, mode or effort does, on a hub whose workspaces have stand-ins for their workers:
//! what the hub saves of it, logs and announces, what it asks of a running agent, and what the next launch is given.
//! What a kind of agent is offered is `hub::agents`' to refuse, and how a launch names a chat's settings is
//! `session::commands`'. The daemon's `e2e/model_and_mode.rs` has a change reaching a real agent, and a refusal in its
//! CLI's words.

use protocol::capability::Control;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Effort;
use protocol::capability::settings::{Effort as Level, Settings};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{AgentKind, ModelName, Seq, Session, SessionId, SetupStatus};
use protocol::prompt::Prompt;
use protocol::worker::{SetupDone, WorkerRequest, WorkerResponse};

use crate::hub::Hub;
use crate::hub::stand_in_workers::{
    Asked, TIMEOUT, a_chat, a_turn, answering_first, hub_with_stand_in_workers, the_agent_says,
};
use crate::hub::tests::claimed;
use crate::session::ChatSettings;

fn in_mode(mode: ApprovalMode) -> Settings {
    Settings { mode: Some(mode), ..Settings::default() }
}

fn on_model(model: &'static str) -> Settings {
    Settings { model: Some(ModelName::named(model)), ..Settings::default() }
}

fn at(effort: Effort) -> Settings {
    Settings { effort: Some(effort), ..Settings::default() }
}

/// What a launch, or a change asked of a running agent, names: `model`, plan mode or full access, and `effort`, by the
/// modes Claude claims, as [`claimed`] says.
fn named(model: &'static str, plan: bool, effort: Level) -> Settings {
    let claims = claimed().approvals.modes.unwrap();
    let mode = if plan { claims.others[0] } else { claims.default };
    Settings { model: Some(ModelName::named(model)), mode: Some(mode), effort: Some(effort), ..Settings::default() }
}

/// Each change of mode the chat's log records, in order.
fn modes_logged(hub: &Hub, chat: &SessionId) -> Vec<ApprovalMode> {
    let log = hub.store.read_events(chat, Seq::FIRST, 1000, u64::MAX, false).unwrap();
    let mode = |body: EventBody| {
        let EventBody::AgentMode { mode } = body else { return None };
        Some(mode)
    };
    log.into_iter().filter_map(|event| mode(event.body)).collect()
}

/// Each change a stand-in was asked to give the running agent of `chat`, in order.
fn changes_asked(asked: &Asked, chat: &SessionId) -> Vec<Settings> {
    asked.requests(|request| {
        let WorkerRequest::AgentControl(asked) = request else { return None };
        let Control::Change(settings) = &asked.control else { return None };
        (asked.session_id == *chat).then(|| settings.clone())
    })
}

/// The newest record of `chat` the feed has told of so far.
fn last_told(feed: &mut crate::hub::Notices, chat: &SessionId) -> Option<Session> {
    std::iter::from_fn(|| feed.try_recv().ok().map(std::sync::Arc::unwrap_or_clone))
        .filter_map(|notice| {
            let HubNotice::Session(told) = notice else { return None };
            (told.id == *chat).then_some(told)
        })
        .last()
}

fn record(hub: &Hub, chat: &SessionId) -> ChatSettings {
    ChatSettings::of(&hub.store.get_session(chat).unwrap().session)
}

/// The agent exits by itself between turns, so the next prompt launches another.
async fn crash(hub: &std::sync::Arc<Hub>, asked: &Asked, chat: &SessionId) {
    let exited =
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: Some(3), signal: None, stderr_tail: vec![] };
    the_agent_says(hub, asked, chat, [exited]).await;
}

/// With no agent running, nothing will say the chat changed mode, so the hub logs each change of mode itself, and only
/// a change. A change leaves what it doesn't set as it was, and the first launch is given what the hub holds.
#[tokio::test]
async fn a_change_with_no_agent_running_is_saved_leaving_the_rest_and_the_first_launch_is_given_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let mut feed = hub.subscribe_feed();

    let planning = hub.session_configure(chat.id.clone(), in_mode(protocol::fixtures::plan_mode())).await.unwrap();
    let told = last_told(&mut feed, &chat.id);
    let on_haiku = hub.session_configure(chat.id.clone(), on_model("haiku")).await.unwrap();
    hub.session_configure(chat.id.clone(), in_mode(protocol::fixtures::plan_mode())).await.unwrap();
    let low = hub.session_configure(chat.id.clone(), at(Effort::Low)).await.unwrap();
    let logged = modes_logged(&hub, &chat.id);
    let launch = a_turn(&hub, &asked, &chat.id, Prompt::new("go"), None).await;

    let described = ChatSettings::of(&chat);
    let plan = ChatSettings { mode: protocol::fixtures::plan_mode(), ..described.clone() };
    assert_eq!(ChatSettings::of(&planning), plan, "the model and the effort as they were");
    assert_eq!(told.as_ref(), Some(&planning), "every client heard of it");
    let haiku = ChatSettings { model: ModelName::named("haiku"), ..plan };
    assert_eq!(ChatSettings::of(&on_haiku), haiku, "the mode as it was");
    let expected = ChatSettings { effort: Effort::Low, ..haiku };
    assert_eq!(ChatSettings::of(&low), expected, "the model and the mode as they were");
    assert_eq!(record(&hub, &chat.id), expected, "as the record says");
    assert_eq!(logged, [protocol::fixtures::plan_mode()], "once, since the others changed no mode");
    assert_eq!(changes_asked(&asked, &chat.id), [], "no agent was running to ask");
    assert_eq!(launch.settings, named("haiku", true, Level::Low));
    assert_eq!(
        modes_logged(&hub, &chat.id),
        [protocol::fixtures::plan_mode()],
        "starting in plan mode isn't a change of mode"
    );
}

/// The running agent is asked to take a change, and it's saved once the agent has, so what's shown is never ahead of
/// what's true. The agent says itself that it has changed mode, so the hub leaves that out of the log. It goes on
/// running, and the next launch is given the change.
#[tokio::test]
async fn a_change_while_the_agent_runs_is_handed_to_it_and_saved_and_the_next_launch_is_given_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("go"), None).await;
    let everything = Settings { model: Some(ModelName::named("haiku")), ..in_mode(protocol::fixtures::plan_mode()) };
    let everything = Settings { effort: Some(Effort::Low), ..everything };

    let changed = hub.session_configure(chat.id.clone(), everything).await.unwrap();
    let launched_by_then = asked.launches(&chat.id).len();
    crash(&hub, &asked, &chat.id).await;
    let next = a_turn(&hub, &asked, &chat.id, Prompt::new("go on"), None).await;

    let expected =
        ChatSettings { model: ModelName::named("haiku"), mode: protocol::fixtures::plan_mode(), effort: Effort::Low };
    assert_eq!(changes_asked(&asked, &chat.id), [named("haiku", true, Level::Low)]);
    assert_eq!((ChatSettings::of(&changed), record(&hub, &chat.id)), (expected.clone(), expected));
    assert_eq!(launched_by_then, 1, "the agent took it as it ran");
    assert_eq!(modes_logged(&hub, &chat.id), [], "the agent says it changed mode, and the hub doesn't");
    assert_eq!(next.settings, named("haiku", true, Level::Low));
}

/// What the running agent refuses isn't saved, and the client is told why in the agent's words, as its worker passes
/// them on. Nor is a change of mode logged, though no agent said it.
#[tokio::test]
async fn a_change_the_running_agent_refuses_is_refused_in_its_words_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    answering_first(&hub, &workspace.id, &asked, |request| {
        let WorkerRequest::AgentControl(asked) = request else { return None };
        let Control::Change(_) = asked.control else { return None };
        Some(Err(ApiError::new(ErrorCode::Worker, "Model 'not-a-model' not found")))
    });
    a_turn(&hub, &asked, &chat.id, Prompt::new("go"), None).await;
    let before = record(&hub, &chat.id);
    let everything = Settings { effort: Some(Effort::Low), ..in_mode(protocol::fixtures::plan_mode()) };
    let everything = Settings { model: Some(ModelName::named("not-a-model")), ..everything };

    let refused = hub.session_configure(chat.id.clone(), everything).await.unwrap_err();
    crash(&hub, &asked, &chat.id).await;
    let next = a_turn(&hub, &asked, &chat.id, Prompt::new("go on"), None).await;

    assert_eq!(refused.code, ErrorCode::Worker, "{refused:?}");
    assert!(refused.message.contains("not found"), "{refused:?}");
    assert_eq!(changes_asked(&asked, &chat.id).len(), 1, "the agent was asked");
    assert_eq!(record(&hub, &chat.id), before, "nothing was saved");
    assert_eq!(modes_logged(&hub, &chat.id), [], "nothing was logged");
    assert_eq!(next.settings, named("sonnet", false, Level::High), "the next launch has what the chat had");
}

/// A prompt waiting on the workspace's setup holds the chat's queue, so a change made meanwhile doesn't go through the
/// queue, or it would wait as long as the setup script runs, and the launch the prompt brings once setup has finished
/// is given the change. The stand-in holds setup until the test lets it go, which only a test on more than one thread
/// can do, and gives up with an error after twice the test's limit, so a test that never lets it go fails rather than
/// hangs.
#[tokio::test(flavor = "multi_thread")]
async fn a_change_made_while_a_prompt_waits_on_setup_is_answered_at_once_and_the_prompts_launch_is_given_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, _) = a_chat(&hub).await;
    let planning = in_mode(protocol::fixtures::plan_mode());
    let chat = hub.session_create(workspace.id.clone(), AgentKind::Claude, planning, None).await.unwrap();
    waiting::task::until(TIMEOUT, "the first setup to finish", || hub.setup_gates.lock().is_empty()).await;
    let (let_go, waiting) = std::sync::mpsc::channel::<()>();
    answering_first(&hub, &workspace.id, &asked, move |request| {
        let WorkerRequest::SetupWorkspace(_) = request else { return None };
        let held = tokio::task::block_in_place(|| waiting.recv_timeout(TIMEOUT * 2));
        let never = |_| ApiError::new(ErrorCode::Worker, "the test never let setup finish");
        Some(held.map(|()| WorkerResponse::SetupDone(SetupDone { code: Some(0) })).map_err(never))
    });
    hub.workspace_rerun_setup(workspace.id.clone()).await.unwrap();
    let (seq, _) = hub.session_prompt(chat.id.clone(), None, Prompt::new("go"), None).await.unwrap();

    let changing = hub.session_configure(chat.id.clone(), in_mode(protocol::fixtures::full_access()));
    let changed = tokio::time::timeout(TIMEOUT, changing).await.expect("the change waited for setup to finish");
    let setup_meanwhile = hub.store.get_workspace(&workspace.id).unwrap().setup;
    let launched_meanwhile = asked.launches(&chat.id).len();
    let_go.send(()).unwrap();
    let given = || asked.prompts_given(&chat.id).contains(&seq);
    waiting::task::until(TIMEOUT, "the prompt to be given once setup had finished", given).await;

    assert_eq!(changed.unwrap().mode, protocol::fixtures::full_access());
    assert_eq!(setup_meanwhile, SetupStatus::Running, "setup was still running as the change was answered");
    assert_eq!(launched_meanwhile, 0, "nothing was launched before setup finished");
    assert_eq!(asked.launches(&chat.id)[0].settings, named("sonnet", false, Level::High));
}
