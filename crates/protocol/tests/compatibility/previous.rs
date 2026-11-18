//! What a hub sends a worker of protocol 3, the version before this one, and what such a worker answers, spelled out,
//! since a worker of an older build carries on with its agents after its hub updates, until its workspace is idle.

use protocol::attachment::AttachmentFile;
use protocol::capability::conversation::Start;
use protocol::capability::settings::Effort;
use protocol::fixtures;
use protocol::frame;
use protocol::model::{AgentKind, AgentLogin, LoginToken, ModelName, RunId, Seq};
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
        settings: previous::Kept {
            model: ModelName::named("opus"),
            permission_mode: fixtures::full_access(),
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

/// A change names what it leaves as `null`, and a mode by the name protocol 3 has for it.
#[test]
fn a_change_keeps_protocol_3s_form() {
    let options = previous::Change { model: Some(ModelName::named("opus")), ..previous::Change::default() };
    sent(
        Request::AgentConfigure(previous::AgentConfigure { session_id: "s".into(), options }),
        r#"{"method":"agent_configure","session_id":"s","model":"opus","permission_mode":null,"effort":null}"#,
    );
    let planning = protocol::capability::settings::Settings {
        mode: Some(fixtures::plan_mode()),
        effort: Some(Effort::Low),
        ..Default::default()
    };
    sent(
        Request::AgentConfigure(previous::AgentConfigure {
            session_id: "s".into(),
            options: previous::Change::of(&planning),
        }),
        r#"{"method":"agent_configure","session_id":"s","model":null,"permission_mode":"plan","effort":"low"}"#,
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

/// A Claude run of protocol 3 that never said what it claims takes the person's answer in the form from before
/// decisions, with answers by each question's text, which every Claude adapter of protocol 3 reads.
#[test]
fn an_answer_goes_to_a_run_of_protocol_3_in_the_form_it_takes() {
    use protocol::capability::approvals::{AfterDenial, Asks, Decision, Remember, Request as Asked};
    use protocol::capability::asking::{Choose, Question, QuestionAnswer, QuestionOption, TypedAnswer};
    use protocol::capability::calls::Call;

    let ask = |asks| Asked { asks, suggested: vec![], always: false, why: None };
    let option = |label: &str| QuestionOption { label: label.into(), description: None, preview: None };
    let question = |text: &str, options| Question {
        text: text.into(),
        header: None,
        options,
        choose: Choose::Several,
        typed: TypedAnswer::Yes,
    };
    let questions = ask(Asks::Questions {
        questions: vec![question("Colors?", vec![option("Red"), option("Blue")]), question("Why?", vec![])],
    });
    let call = ask(Asks::Call { call: Call::Advisor });
    let answer = |decision: &Decision, request: &Asked| {
        previous::AgentApproval::of("s".into(), "a".into(), decision, request).map(previous::Request::AgentApproval)
    };
    let picked = QuestionAnswer { picked: vec![0, 1], typed: Some("Teal".into()) };
    let answered = Decision::Answer { answers: vec![picked, QuestionAnswer::default()] };
    let stopped = Decision::Deny { words: Some("No".into()), then: AfterDenial::Stop, keep: None };

    sent(
        answer(&Decision::Allow { remember: Remember::Once }, &call).unwrap(),
        r#"{"method":"agent_approval","session_id":"s","approval_id":"a","decision":{"decision":"allow"}}"#,
    );
    sent(
        answer(&stopped, &call).unwrap(),
        r#"{"method":"agent_approval","session_id":"s","approval_id":"a","decision":{"decision":"deny","message":"No","interrupt":true}}"#,
    );
    sent(
        answer(&answered, &questions).unwrap(),
        r#"{"method":"agent_approval","session_id":"s","approval_id":"a","decision":{"decision":"answer","answers":{"Colors?":"Red, Blue, Teal"}}}"#,
    );
    // Questions the person chose not to answer, which protocol 3 says as a denial.
    let skipped = Decision::Answer { answers: vec![QuestionAnswer::default(), QuestionAnswer::default()] };
    sent(
        answer(&skipped, &questions).unwrap(),
        r#"{"method":"agent_approval","session_id":"s","approval_id":"a","decision":{"decision":"deny","interrupt":false}}"#,
    );
    let for_the_conversation = Decision::Allow { remember: Remember::Conversation };
    assert_eq!(
        answer(&for_the_conversation, &call),
        None,
        "a use allowed for the conversation, which it has no form for"
    );
}
