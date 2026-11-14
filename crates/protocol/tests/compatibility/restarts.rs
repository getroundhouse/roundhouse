//! What lets a worker outlive its hub, which a hub and a worker from either side of it have to read alike: a hub from
//! before it says nothing of it, and one from after has to tell whether its worker or its hub does.

use protocol::client::{ClientFrame, ClientRequest, HubStatus};
use protocol::frame;
use protocol::model::{LimitWindow, ModelLabel, RunId};
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
        frame::decode(br#"{"result":"running_agents","agents":[{"session_id":"s","run":4}]}"#).unwrap();
    let running = RunningAgent { session_id: "s".into(), run: RunId::new(4), capabilities: None };
    assert_eq!(reply, WorkerResponse::RunningAgents(RunningAgentsReply { agents: vec![running] }));
}

/// Numbered for a hub that asked the worker to keep running without it, and not otherwise, which a hub from before
/// numbering still reads.
#[test]
fn what_a_worker_sends_keeps_its_form_numbered_and_not() {
    let numbered = WorkerFrame::AgentLimits {
        session_id: "s".into(),
        run: RunId::new(4),
        limits: vec![],
        every_models_week: false,
        seq: Some(9),
    };
    let line = br#"{"frame":"agent_limits","session_id":"s","run":4,"limits":[],"seq":9}"#;
    assert_eq!(frame::encode(&numbered).unwrap(), [&line[..], b"\n"].concat());

    let unnumbered: WorkerFrame =
        frame::decode(br#"{"frame":"agent_limits","session_id":"s","run":4,"limits":[]}"#).unwrap();
    let WorkerFrame::AgentLimits { seq, every_models_week, .. } = unnumbered else { panic!("{unnumbered:?}") };
    assert_eq!((seq, every_models_week), (None, false), "a worker from before says nothing of models' weeks");
}

/// A worker that lists every model's week says so, with each by its model.
#[test]
fn a_worker_that_lists_every_models_week_says_so_and_keeps_its_form() {
    let line = br#"{"frame":"agent_limits","session_id":"s","run":4,"limits":[{"window":"model_seven_days","model":"Fable","used_percent":88,"resets_at_ms":9,"heard_at_ms":null}],"every_models_week":true}"#;
    let frame: WorkerFrame = frame::decode(line).unwrap();

    let WorkerFrame::AgentLimits { limits, every_models_week, .. } = &frame else { panic!("{frame:?}") };
    assert!(every_models_week);
    assert_eq!(
        limits.iter().map(|limit| (limit.window, limit.model.clone(), limit.used_percent)).collect::<Vec<_>>(),
        [(LimitWindow::ModelSevenDays, Some(ModelLabel::new("Fable")), 88)]
    );
    assert_eq!(frame::encode(&frame).unwrap(), [&line[..], b"\n"].concat());
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
