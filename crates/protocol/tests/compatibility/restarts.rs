//! What lets a worker outlive its hub, which a hub and a worker from either side of it have to read alike: a hub from
//! before it says nothing of it, and one from after has to tell whether its worker or its hub does.

use protocol::capability::Report;
use protocol::client::{ClientFrame, ClientRequest, HubStatus};
use protocol::frame;
use protocol::model::{ModelLabel, RunId};
use protocol::tail::Tail;
use protocol::worker::{
    Received, RunningAgent, RunningAgentsReply, WorkerCall, WorkerFrame, WorkerRequest, WorkerResponse,
};

#[test]
fn what_a_hub_asks_of_a_worker_it_reconnects_to_keeps_its_form() {
    let calls = [
        (WorkerRequest::Stop, &br#"{"id":1,"request":{"method":"stop"}}"#[..]),
        (WorkerRequest::RunningAgents, br#"{"id":1,"request":{"method":"running_agents"}}"#),
        (
            WorkerRequest::Received(Received { through: 12 }),
            br#"{"id":1,"request":{"method":"received","through":12}}"#,
        ),
    ];
    for (request, line) in calls {
        assert_eq!(frame::encode(&WorkerCall { id: 1, request }).unwrap(), [line, b"\n"].concat());
    }

    let reply: WorkerResponse =
        frame::decode(br#"{"result":"running_agents","agents":[{"session_id":"s","run":4,"capabilities":{}}]}"#)
            .unwrap();
    let running = RunningAgent { session_id: "s".into(), run: RunId::new(4), capabilities: Box::default() };
    assert_eq!(reply, WorkerResponse::RunningAgents(RunningAgentsReply { agents: vec![running] }));
}

/// Numbered for a hub that asked the worker to keep running without it, and not otherwise, which a hub from before
/// numbering still reads.
#[test]
fn what_a_worker_sends_keeps_its_form_numbered_and_not() {
    let report = Report::Limits { limits: vec![], every_models_week: false };
    let numbered = WorkerFrame::AgentReport { session_id: "s".into(), run: RunId::new(4), report, seq: Some(9) };
    let line = br#"{"frame":"agent_report","session_id":"s","run":4,"report":{"report":"limits","limits":[]},"seq":9}"#;
    assert_eq!(frame::encode(&numbered).unwrap(), [&line[..], b"\n"].concat());

    let unnumbered: WorkerFrame =
        frame::decode(br#"{"frame":"agent_report","session_id":"s","run":4,"report":{"report":"limits","limits":[]}}"#)
            .unwrap();
    let WorkerFrame::AgentReport { seq, .. } = unnumbered else { panic!("{unnumbered:?}") };
    assert_eq!(seq, None);
}

/// A worker that lists every model's week says so, with each by its model and the minutes it counts over.
#[test]
fn a_worker_that_lists_every_models_week_says_so_and_keeps_its_form() {
    let line = br#"{"frame":"agent_report","session_id":"s","run":4,"report":{"report":"limits","limits":[{"window_minutes":10080,"model":"Fable","used_percent":88,"resets_at_ms":9}],"every_models_week":true}}"#;
    let frame: WorkerFrame = frame::decode(line).unwrap();

    let WorkerFrame::AgentReport { report: Report::Limits { limits, every_models_week }, .. } = &frame else {
        panic!("{frame:?}")
    };
    assert!(every_models_week);
    assert_eq!(
        limits.iter().map(|limit| (limit.window_minutes, limit.model.clone(), limit.used_percent)).collect::<Vec<_>>(),
        [(Some(10_080), Some(ModelLabel::new("Fable")), 88)]
    );
    assert_eq!(frame::encode(&frame).unwrap(), [&line[..], b"\n"].concat());
}

/// A report a newer worker sends that this build has no kind for still reads, as one it doesn't know, which the hub
/// drops, rather than failing the frame.
#[test]
fn a_report_this_build_doesnt_know_reads_as_unknown() {
    let line =
        br#"{"frame":"agent_report","session_id":"s","run":4,"report":{"report":"weather","sky":"blue"},"seq":2}"#;
    let frame: WorkerFrame = frame::decode(line).unwrap();

    let WorkerFrame::AgentReport { report, seq, .. } = frame else { panic!("{frame:?}") };
    assert_eq!((report, seq), (Report::Unknown, Some(2)));
}

/// What an agent is writing, as a worker sends it to a hub that connects.
#[test]
fn what_a_worker_says_an_agent_is_writing_keeps_its_form() {
    let block = r#"{"index":0,"kind":"thinking","tool_name":null,"text":"Reading the"}"#;
    let message = format!(r#"{{"run":4,"message_id":"m","parent_tool_use_id":null,"blocks":[{block}]}}"#);
    let line = format!(r#"{{"frame":"agent_tail","session_id":"s","run":4,"tail":{{"messages":[{message}]}}}}"#);

    let read: WorkerFrame = frame::decode(line.as_bytes()).unwrap();

    let WorkerFrame::AgentTail { session_id, run, tail } = &read else { panic!("{read:?}") };
    assert_eq!((session_id.as_str(), *run), ("s", RunId::new(4)));
    assert_eq!(tail.messages[0].blocks[0].text, "Reading the");
    assert_eq!(frame::encode(&read).unwrap(), [line.as_bytes(), b"\n"].concat());
}

/// An agent trying a request again, as a worker sends it, and a tail that holds it, as a worker sends one to a hub
/// that connects and a hub sends one to a client. A tail from before says nothing of it.
#[test]
fn what_a_worker_says_of_a_request_tried_again_keeps_its_form() {
    let retrying = r#"{"attempt":2,"of":10,"status":503,"delay_ms":1204}"#;
    let line = format!(r#"{{"frame":"agent_retrying","session_id":"s","run":4,"retrying":{retrying}}}"#);
    let kept =
        format!(r#"{{"frame":"agent_tail","session_id":"s","run":4,"tail":{{"messages":[],"retrying":{retrying}}}}}"#);

    let read: WorkerFrame = frame::decode(line.as_bytes()).unwrap();
    let in_tail: WorkerFrame = frame::decode(kept.as_bytes()).unwrap();

    let WorkerFrame::AgentRetrying { run, retrying, .. } = &read else { panic!("{read:?}") };
    assert_eq!(*run, RunId::new(4));
    assert_eq!(
        (retrying.attempt, retrying.of, retrying.status, retrying.delay_ms),
        (2, Some(10), Some(503), Some(1_204))
    );
    assert_eq!(frame::encode(&read).unwrap(), [line.as_bytes(), b"\n"].concat());
    let WorkerFrame::AgentTail { tail, .. } = &in_tail else { panic!("{in_tail:?}") };
    assert_eq!(tail.retrying.as_ref(), Some(retrying));
    assert_eq!(frame::encode(&in_tail).unwrap(), [kept.as_bytes(), b"\n"].concat());
    let before: Tail = serde_json::from_str(r#"{"messages":[]}"#).unwrap();
    assert_eq!(before.retrying, None);
}

/// A restart works without a hello, as a stop does, so a client can restart a hub it can't otherwise talk to, once its
/// status says the hub knows how.
#[test]
fn a_restart_and_what_says_a_hub_knows_one_keep_their_form() {
    let restart = ClientFrame { id: 2, request: ClientRequest::HubRestart };
    assert_eq!(frame::encode(&restart).unwrap(), b"{\"id\":2,\"request\":{\"method\":\"hub_restart\"}}\n");

    let fields = r#""pid":7,"hub_version":"a","protocol_version":4,"started_at_ms":1,"workers":4,"running_sessions":2,"clients":1,"on_demand":true"#;
    let now: HubStatus =
        serde_json::from_str(&format!(r#"{{{fields},"keeps_workers":true,"oldest_protocol_version":3}}"#)).unwrap();
    assert_eq!((now.keeps_workers, now.oldest_protocol_version), (true, Some(3)));
    let before: HubStatus = serde_json::from_str(&format!("{{{fields}}}")).unwrap();
    assert_eq!((before.keeps_workers, before.oldest_protocol_version), (false, None), "from a hub from before");
}
