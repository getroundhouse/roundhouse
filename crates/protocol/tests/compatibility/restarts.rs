//! What lets a worker outlive its hub, which a hub and a worker from either side of it have to read alike: a hub from
//! before it says nothing of it, and one from after has to tell whether its worker or its hub does.

use protocol::client::{ClientFrame, ClientRequest, HubStatus};
use protocol::frame;
use protocol::model::RunId;
use protocol::worker::{RunningAgent, WorkerCall, WorkerFrame, WorkerRequest, WorkerResponse};

#[test]
fn what_a_hub_asks_of_a_worker_it_reconnects_to_keeps_its_form() {
    let calls = [
        (WorkerRequest::Stop, &br#"{"id":1,"request":{"method":"stop"}}"#[..]),
        (WorkerRequest::RunningAgents, br#"{"id":1,"request":{"method":"running_agents"}}"#),
        (WorkerRequest::Received { through: 12 }, br#"{"id":1,"request":{"method":"received","through":12}}"#),
    ];
    for (request, line) in calls {
        assert_eq!(frame::encode(&WorkerCall { id: 1, request }).unwrap(), [line, b"\n"].concat());
    }

    let reply: WorkerResponse =
        frame::decode(br#"{"result":"running_agents","agents":[{"session_id":"s","run":4}]}"#).unwrap();
    let running = RunningAgent { session_id: "s".into(), run: RunId::new(4) };
    assert_eq!(reply, WorkerResponse::RunningAgents { agents: vec![running] });
}

/// Numbered for a hub that asked the worker to keep running without it, and not otherwise, which a hub from before
/// numbering still reads.
#[test]
fn what_a_worker_sends_keeps_its_form_numbered_and_not() {
    let numbered =
        WorkerFrame::AgentLimits { session_id: "s".into(), run: RunId::new(4), limits: vec![], seq: Some(9) };
    let line = br#"{"frame":"agent_limits","session_id":"s","run":4,"limits":[],"seq":9}"#;
    assert_eq!(frame::encode(&numbered).unwrap(), [&line[..], b"\n"].concat());

    let unnumbered: WorkerFrame =
        frame::decode(br#"{"frame":"agent_limits","session_id":"s","run":4,"limits":[]}"#).unwrap();
    let WorkerFrame::AgentLimits { seq, .. } = unnumbered else { panic!("{unnumbered:?}") };
    assert_eq!(seq, None);
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
