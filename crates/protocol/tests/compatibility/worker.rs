//! What has to stay readable between a hub and its workers.

use protocol::frame::{self, Call};
use protocol::model::{Wait, WaitEnd, WaitEnded, WaitUntil};
use protocol::worker::Request as _;
use protocol::worker::{
    AskAnswer, AskId, AskOutcome, AskReply, CancelWait, PrepareWorkspace, PullRequestAnswer, ReadWait,
    RestoreWorkspace, ServeTools, StartWait, ToWorker, ToolsServing, WaitAnswer, WorkerAsk, WorkerCall, WorkerFrame,
    WorkerOutcome, WorkerRequest, WorkerResponse, WorkspaceReady, WorktreeThere, WorktreeThereReply,
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
    let written = frame::encode(&ask).unwrap();
    assert_eq!(written, b"{\"frame\":\"ask\",\"id\":3,\"session_id\":\"s\",\"ask\":{\"method\":\"pull_request\"}}\n");
    assert_eq!(frame::decode::<WorkerFrame>(&written).unwrap(), ask, "and it's read the way it's written");

    let answer = br#"{"answer":3,"response":{"result":"pull_request","pull_request":null}}"#;
    let ToWorker::Answer(answered) = ToWorker::decode(answer).unwrap() else {
        panic!("an answer reads as an answer");
    };
    assert_eq!(
        answered,
        AskAnswer {
            answer: AskId::new(3),
            outcome: AskOutcome::Response(AskReply::PullRequest(PullRequestAnswer { pull_request: None }))
        }
    );
    assert_eq!(serde_json::to_vec(&answered).unwrap(), answer, "and it's written the way it's read");

    let serving: WorkerCall = frame::decode(br#"{"id":5,"request":{"method":"serve_tools","port":50123}}"#).unwrap();
    assert_eq!(serving.request, WorkerRequest::ServeTools(ServeTools { port: Some(50_123) }));
    let reply: WorkerFrame =
        frame::decode(br#"{"frame":"reply","id":5,"response":{"result":"tools_serving","port":50123}}"#).unwrap();
    assert_eq!(
        reply,
        WorkerFrame::Reply {
            id: 5,
            outcome: WorkerOutcome::Response(WorkerResponse::ToolsServing(ToolsServing { port: 50_123 }))
        }
    );
}

/// An ask with no fields reads with fields a newer worker added to it, as a request with none does.
#[test]
fn an_ask_with_no_fields_reads_with_fields_it_doesnt_know() {
    let newer: WorkerAsk = serde_json::from_str(r#"{"method":"pull_request","newer":1}"#).unwrap();
    assert_eq!(newer, WorkerAsk::PullRequest);
}

/// A worker's asks for a wait, and the hub's answer with one, spelled out.
#[test]
fn what_a_worker_asks_of_a_wait_and_its_hubs_answer_keep_their_form() {
    let keeps_its_form = |ask: WorkerAsk, wire: &str| {
        assert_eq!(serde_json::to_string(&ask).unwrap(), wire);
        assert_eq!(serde_json::from_str::<WorkerAsk>(wire).unwrap(), ask);
    };
    keeps_its_form(
        WorkerAsk::StartWait(StartWait { until: WaitUntil::Checks, call: Some("toolu_1".into()) }),
        r#"{"method":"start_wait","until":"checks","call":"toolu_1"}"#,
    );
    let until_done = WorkerAsk::StartWait(StartWait { until: WaitUntil::Done, call: None });
    keeps_its_form(until_done.clone(), r#"{"method":"start_wait","until":"done"}"#);
    let unsaid: WorkerAsk = serde_json::from_str(r#"{"method":"start_wait"}"#).unwrap();
    assert_eq!(unsaid, until_done, "until the pull request is done");
    keeps_its_form(WorkerAsk::Wait(ReadWait { wait: "w".into() }), r#"{"method":"wait","wait":"w"}"#);
    keeps_its_form(WorkerAsk::CancelWait(CancelWait { wait: "w".into() }), r#"{"method":"cancel_wait","wait":"w"}"#);

    let answer = br#"{"answer":4,"response":{"result":"wait","wait":{"id":"w","session_id":"s","workspace_id":"v","number":12,"until":"done","started_at_ms":5,"ended":{"at_ms":9,"how":"merged"}},"pull_request":null}}"#;
    let wait = Wait {
        id: "w".into(),
        session_id: "s".into(),
        workspace_id: "v".into(),
        number: 12,
        call: None,
        until: WaitUntil::Done,
        watch: None,
        started_at_ms: 5,
        ended: Some(WaitEnded { at_ms: 9, how: WaitEnd::Merged }),
    };
    let ToWorker::Answer(answered) = ToWorker::decode(answer).unwrap() else {
        panic!("an answer reads as an answer");
    };
    assert_eq!(
        answered,
        AskAnswer {
            answer: AskId::new(4),
            outcome: AskOutcome::Response(AskReply::Wait(WaitAnswer { wait, pull_request: None }))
        }
    );
    assert_eq!(serde_json::to_vec(&answered).unwrap(), answer, "and it's written the way it's read");
}

/// A branch is a type of its own, and a plain string on the wire, as it was before it had one.
#[test]
fn a_branch_keeps_its_form_in_what_a_hub_and_its_worker_send() {
    let keeps_its_form = |call: WorkerCall, line: &str| {
        assert_eq!(String::from_utf8(frame::encode(&call).unwrap()).unwrap(), format!("{line}\n"));
        assert_eq!(frame::decode::<WorkerCall>(line.as_bytes()).unwrap(), call);
    };
    let prepare = WorkerRequest::PrepareWorkspace(PrepareWorkspace {
        repo_name: "r".into(),
        repo_path: "/r".into(),
        workspace_name: "aspen".into(),
        branch: "aspen".into(),
        base_ref: Some("origin/main".into()),
    });
    keeps_its_form(
        WorkerCall { id: 1, request: prepare },
        r#"{"id":1,"request":{"method":"prepare_workspace","repo_name":"r","repo_path":"/r","workspace_name":"aspen","branch":"aspen","base_ref":"origin/main"}}"#,
    );
    let restore = WorkerRequest::RestoreWorkspace(RestoreWorkspace {
        repo_path: "/r".into(),
        path: "/w".into(),
        branch: "birch".into(),
    });
    keeps_its_form(
        WorkerCall { id: 2, request: restore },
        r#"{"id":2,"request":{"method":"restore_workspace","repo_path":"/r","path":"/w","branch":"birch"}}"#,
    );

    keeps_its_form(
        WorkerCall { id: 4, request: WorktreeThere { path: "/w".into() }.into_request() },
        r#"{"id":4,"request":{"method":"worktree_there","path":"/w"}}"#,
    );
    let there: WorkerFrame =
        frame::decode(br#"{"frame":"reply","id":4,"response":{"result":"worktree_there","there":true}}"#).unwrap();
    let WorkerFrame::Reply {
        outcome: WorkerOutcome::Response(WorkerResponse::WorktreeThere(WorktreeThereReply { there })),
        ..
    } = there
    else {
        panic!("{there:?}");
    };
    assert!(there);

    let ready: WorkerFrame = frame::decode(
        br#"{"frame":"reply","id":1,"response":{"result":"workspace_ready","path":"/w","branch":"aspen","base_ref":"origin/main"}}"#,
    )
    .unwrap();
    let status: WorkerFrame = frame::decode(
        br#"{"frame":"reply","id":3,"response":{"result":"git_status","branch":"birch","head":null,"ahead":0,"behind":0,"files":[]}}"#,
    )
    .unwrap();

    let WorkerFrame::Reply {
        outcome: WorkerOutcome::Response(WorkerResponse::WorkspaceReady(WorkspaceReady { branch, .. })),
        ..
    } = ready
    else {
        panic!("{ready:?}");
    };
    assert_eq!(branch.as_str(), "aspen");
    let WorkerFrame::Reply { outcome: WorkerOutcome::Response(WorkerResponse::GitStatus(status)), .. } = status else {
        panic!("{status:?}");
    };
    assert_eq!(status.branch, Some("birch".into()));
}
