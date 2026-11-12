//! What has to stay readable between a hub and its workers.

use protocol::frame::{self, Call};
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
