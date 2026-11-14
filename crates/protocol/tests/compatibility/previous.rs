//! What a hub sends a worker of protocol 3, the version before this one, and what such a worker answers, spelled out,
//! since a worker of an older build carries on with its agents after its hub updates, until its workspace is idle.

use protocol::attachment::AttachmentFile;
use protocol::capability::conversation::Start;
use protocol::frame;
use protocol::model::{
    AgentKind, AgentLogin, Effort, LoginToken, ModelName, PermissionMode, RunId, Seq, SessionSettings,
    SessionSettingsChange,
};
use protocol::prompt::Prompt;
use protocol::worker::previous::{self, AgentStart, Call, Request};
use protocol::worker::{self, ToolsToken, WorkerResponse};

use super::prompts::log;

/// `request` is written as `{"id":7,"request":<form>}`, as protocol 3 has it.
fn sent(request: Request, form: &str) {
    let line = String::from_utf8(frame::encode(&Call { id: 7, request }).unwrap()).unwrap();
    assert_eq!(line, format!("{{\"id\":7,\"request\":{form}}}\n"));
}

fn launch(start: Start) -> AgentStart {
    AgentStart {
        session_id: "s".into(),
        run: RunId::new(4),
        agent: AgentKind::Claude,
        cwd: "/w".into(),
        start,
        settings: SessionSettings {
            model: ModelName::named("opus"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::High,
        },
        env: vec![],
        tools_token: Some(ToolsToken::new("t")),
        login: None,
    }
}

#[test]
fn every_request_about_an_agent_keeps_protocol_3s_form() {
    let resumed = launch(Start::Resume { conversation: "a".into() });
    sent(
        resumed.clone().into_request(),
        r#"{"method":"agent_start","session_id":"s","run":4,"agent":"claude","cwd":"/w","agent_session_id":"a","resume":true,"model":"opus","permission_mode":"bypassPermissions","effort":"high","env":[],"tools_token":"t"}"#,
    );
    let under_a_login =
        AgentStart { tools_token: None, login: Some(AgentLogin { token: LoginToken::new("made-up") }), ..resumed };
    sent(
        under_a_login.into_request(),
        r#"{"method":"agent_start_under_login","session_id":"s","run":4,"agent":"claude","cwd":"/w","agent_session_id":"a","resume":true,"model":"opus","permission_mode":"bypassPermissions","effort":"high","env":[],"login":{"token":"made-up"}}"#,
    );
    let chosen_by_the_agent = AgentStart { agent: AgentKind::Codex, ..launch(Start::New { conversation: None }) };
    sent(
        chosen_by_the_agent.into_request(),
        r#"{"method":"agent_start","session_id":"s","run":4,"agent":"codex","cwd":"/w","start":"new","model":"opus","permission_mode":"bypassPermissions","effort":"high","env":[],"tools_token":"t"}"#,
    );

    let file = AttachmentFile { attachment: log(), path: "/w/.context/attachments/build.log".into() };
    let prompt = Prompt::carrying("why?", vec![file]);
    let attached = r#"{"method":"agent_prompt","session_id":"s","prompt":3,"text":"why?","attachments":[{"attachment":{"id":"a1","name":"build.log","media_type":"text/plain","size_bytes":3},"path":"/w/.context/attachments/build.log"}]}"#;
    sent(Request::AgentPrompt(previous::AgentPrompt { session_id: "s".into(), seq: Seq::new(3), prompt }), attached);
    sent(
        Request::AgentPrompt(previous::AgentPrompt {
            session_id: "s".into(),
            seq: Seq::new(3),
            prompt: Prompt::new("go"),
        }),
        r#"{"method":"agent_prompt","session_id":"s","prompt":3,"text":"go"}"#,
    );
    sent(
        Request::AgentCancel(previous::AgentCancel { session_id: "s".into() }),
        r#"{"method":"agent_cancel","session_id":"s"}"#,
    );
    sent(
        Request::AgentCompact(previous::AgentCompact {
            session_id: "s".into(),
            asked: Seq::new(9),
            keeping: Some("The API".into()),
        }),
        r#"{"method":"agent_compact","session_id":"s","asked":9,"keeping":"The API"}"#,
    );
    let options = SessionSettingsChange { model: Some(ModelName::named("opus")), permission_mode: None, effort: None };
    sent(
        Request::AgentConfigure(previous::AgentConfigure { session_id: "s".into(), options }),
        r#"{"method":"agent_configure","session_id":"s","model":"opus","permission_mode":null,"effort":null}"#,
    );
    let title = worker::TitleFor {
        agent: AgentKind::Claude,
        prompt: "Fix this".into(),
        images: vec![],
        login: Some(AgentLogin { token: LoginToken::new("made-up") }),
    };
    sent(
        Request::TitleUnderLogin(title),
        r#"{"method":"title_under_login","agent":"claude","prompt":"Fix this","login":{"token":"made-up"}}"#,
    );
}

#[test]
fn what_a_worker_of_protocol_3_answers_reads_as_claiming_nothing() {
    let started: WorkerResponse =
        frame::decode(br#"{"result":"agent_started","pid":4242,"run":4,"already_running":false}"#).unwrap();
    let WorkerResponse::AgentStarted(worker::AgentStarted { capabilities: None, .. }) = started else {
        panic!("{started:?}")
    };
    let running: WorkerResponse =
        frame::decode(br#"{"result":"running_agents","agents":[{"session_id":"s","run":4}]}"#).unwrap();
    let WorkerResponse::RunningAgents(worker::RunningAgentsReply { agents }) = running else { panic!("{running:?}") };
    assert_eq!(agents[0].capabilities, None);
}
