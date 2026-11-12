//! What has to stay readable between a hub and its workers.

use protocol::frame::{self, Call};
use protocol::model::{Wait, WaitEnd, WaitEnded, WaitUntil};
use protocol::worker::{
    AskAnswer, AskId, AskOutcome, AskReply, ToWorker, WorkerAsk, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest,
    WorkerResponse,
};

#[test]
fn an_agent_started_by_a_hub_from_before_the_tools_has_no_token_for_them() {
    let line = br#"{"id":4,"request":{"method":"agent_start","session_id":"s","run":1,"agent":"claude","cwd":"/w","agent_session_id":"a","resume":false,"model":"opus","permission_mode":"bypassPermissions","effort":"high","env":[]}}"#;

    let ToWorker::Call(Call { request: Ok(WorkerRequest::AgentStart(spec)), .. }) = ToWorker::decode(line).unwrap()
    else {
        panic!("a start reads as a start");
    };

    assert_eq!(spec.tools_token, None);
}

/// What the worker asks for its tools and the hub's answers, spelled out, since they cross between processes.
#[test]
fn what_a_worker_asks_and_what_its_hub_answers_keep_their_form() {
    let ask = WorkerFrame::Ask { id: AskId::new(3), session_id: "s".into(), ask: WorkerAsk::PullRequest };
    assert_eq!(
        frame::encode(&ask).unwrap(),
        b"{\"frame\":\"ask\",\"id\":3,\"session_id\":\"s\",\"ask\":{\"method\":\"pull_request\"}}\n"
    );

    let answer = br#"{"answer":3,"response":{"result":"pull_request","pull_request":null}}"#;
    let ToWorker::Answer(answered) = ToWorker::decode(answer).unwrap() else {
        panic!("an answer reads as an answer");
    };
    assert_eq!(
        answered,
        AskAnswer {
            answer: AskId::new(3),
            outcome: AskOutcome::Response(AskReply::PullRequest { pull_request: None })
        }
    );

    let serving: WorkerCall = frame::decode(br#"{"id":5,"request":{"method":"serve_tools","port":50123}}"#).unwrap();
    assert_eq!(serving.request, WorkerRequest::ServeTools { port: Some(50_123) });
    let reply: WorkerFrame =
        frame::decode(br#"{"frame":"reply","id":5,"response":{"result":"tools_serving","port":50123}}"#).unwrap();
    assert_eq!(
        reply,
        WorkerFrame::Reply { id: 5, outcome: WorkerOutcome::Response(WorkerResponse::ToolsServing { port: 50_123 }) }
    );
}

/// A worker's asks for a wait, and the hub's answer with one, spelled out.
#[test]
fn what_a_worker_asks_of_a_wait_and_its_hubs_answer_keep_their_form() {
    let start = WorkerAsk::StartWait { until: WaitUntil::Checks, call: Some("toolu_1".into()) };
    assert_eq!(serde_json::to_string(&start).unwrap(), r#"{"method":"start_wait","until":"checks","call":"toolu_1"}"#);
    let unsaid: WorkerAsk = serde_json::from_str(r#"{"method":"start_wait"}"#).unwrap();
    assert_eq!(unsaid, WorkerAsk::StartWait { until: WaitUntil::Done, call: None }, "until the pull request is done");
    let read: WorkerAsk = serde_json::from_str(r#"{"method":"wait","wait":"w"}"#).unwrap();
    let cancel: WorkerAsk = serde_json::from_str(r#"{"method":"cancel_wait","wait":"w"}"#).unwrap();
    assert_eq!((read, cancel), (WorkerAsk::Wait { wait: "w".into() }, WorkerAsk::CancelWait { wait: "w".into() }));

    let answer = br#"{"answer":4,"response":{"result":"wait","wait":{"id":"w","session_id":"s","workspace_id":"v","number":12,"until":"done","started_at_ms":5,"ended":{"at_ms":9,"how":"merged"}},"pull_request":null}}"#;
    let wait = Wait {
        id: "w".into(),
        session_id: "s".into(),
        workspace_id: "v".into(),
        number: 12,
        call: None,
        until: WaitUntil::Done,
        started_at_ms: 5,
        ended: Some(WaitEnded { at_ms: 9, how: WaitEnd::Merged }),
    };
    let ToWorker::Answer(answered) = ToWorker::decode(answer).unwrap() else {
        panic!("an answer reads as an answer");
    };
    assert_eq!(
        answered,
        AskAnswer { answer: AskId::new(4), outcome: AskOutcome::Response(AskReply::Wait { wait, pull_request: None }) }
    );
}
