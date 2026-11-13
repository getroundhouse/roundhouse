//! Every message about a terminal, spelled out, since a client, its hub and the hub's workers can each be a different
//! build, and a worker outlives the hub that started it.

use protocol::client::terminals::{TerminalAttach, TerminalClose, TerminalInput, TerminalOpen, TerminalResize};
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, HubNotice, HubStatus, ReplyOutcome};
use protocol::frame;
use protocol::model::{RecordKey, Records, Rev, Terminal};
use protocol::terminal::{TerminalBytes, TerminalExit, TerminalScreen, TerminalSize, TerminalUpdate, UpdateIndex};
use protocol::worker::{
    self, RunningTerminal, RunningTerminalsReply, TerminalShown, WorkerFrame, WorkerRequest, WorkerResponse,
};

use crate::keeps_its_form;
use crate::worker_calls::{answered, asked};

const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };

fn terminal() -> Terminal {
    Terminal { id: "t".into(), workspace_id: "w".into(), opened_at_ms: 5, rev: Rev::new(9) }
}

#[test]
fn every_request_a_client_makes_of_a_terminal_keeps_its_form() {
    let client_asked = |request: ClientRequest, form: &str| {
        keeps_its_form(&ClientFrame { id: 7, request }, &format!(r#"{{"id":7,"request":{form}}}"#));
    };

    let open = TerminalOpen { workspace_id: "w".into(), id: "t".into(), size: SIZE };
    client_asked(
        ClientRequest::TerminalOpen(open),
        r#"{"method":"terminal_open","workspace_id":"w","id":"t","size":{"cols":80,"rows":24}}"#,
    );
    let attach = TerminalAttach { id: "t".into(), subscription_id: 3 };
    client_asked(ClientRequest::TerminalAttach(attach), r#"{"method":"terminal_attach","id":"t","subscription_id":3}"#);
    let input = TerminalInput { id: "t".into(), input: b"ls\r".as_slice().into() };
    client_asked(ClientRequest::TerminalInput(input), r#"{"method":"terminal_input","id":"t","input":"bHMN"}"#);
    let resize = TerminalResize { id: "t".into(), size: TerminalSize { cols: 120, rows: 40 } };
    client_asked(
        ClientRequest::TerminalResize(resize),
        r#"{"method":"terminal_resize","id":"t","size":{"cols":120,"rows":40}}"#,
    );
    client_asked(
        ClientRequest::TerminalClose(TerminalClose { id: "t".into() }),
        r#"{"method":"terminal_close","id":"t"}"#,
    );
}

#[test]
fn a_terminals_record_keeps_its_form_as_a_reply_a_notice_and_a_removal() {
    let record = r#""id":"t","workspace_id":"w","opened_at_ms":5,"rev":9"#;

    let reply = HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(ClientResponse::Terminal(terminal())) };
    keeps_its_form(&reply, &format!(r#"{{"frame":"reply","id":7,"response":{{"result":"terminal",{record}}}}}"#));
    let notice = HubFrame::Notice { subscription_id: 1, notice: HubNotice::Terminal(terminal()) };
    keeps_its_form(
        &notice,
        &format!(r#"{{"frame":"notice","subscription_id":1,"notice":{{"notice":"terminal",{record}}}}}"#),
    );
    let removed = HubNotice::Removed { record: RecordKey::Terminal { id: "t".into() }, rev: Rev::new(10) };
    keeps_its_form(&removed, r#"{"notice":"removed","record":{"kind":"terminal","id":"t"},"rev":10}"#);
}

/// A hub from before terminals lists none, and a list with none has the form it had before, for a client from then.
#[test]
fn a_list_of_records_has_its_terminals_and_reads_without_them() {
    let before: Records =
        serde_json::from_str(r#"{"repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":4}"#).unwrap();
    assert_eq!(before.terminals, Vec::new());
    assert!(!serde_json::to_string(&before).unwrap().contains("terminals"));

    let with = Records { terminals: vec![terminal()], ..Records::default() };
    let line = serde_json::to_string(&with).unwrap();
    assert!(line.contains(r#""terminals":[{"id":"t","workspace_id":"w","opened_at_ms":5,"rev":9}]"#), "{line}");
    assert_eq!(serde_json::from_str::<Records>(&line).unwrap(), with);
}

#[test]
fn every_update_a_hub_sends_a_client_of_a_terminal_keeps_its_form() {
    let sent = |update: TerminalUpdate, form: &str| {
        let frame = HubFrame::Terminal { subscription_id: 3, update };
        keeps_its_form(&frame, &format!(r#"{{"frame":"terminal","subscription_id":3,"update":{form}}}"#));
    };

    let screen = TerminalScreen { size: SIZE, bytes: b"\x1b[2J\x1b[Hhi".as_slice().into() };
    sent(TerminalUpdate::Screen(screen), r#"{"update":"screen","size":{"cols":80,"rows":24},"bytes":"G1syShtbSGhp"}"#);
    let output = TerminalBytes::from(b"\x1b[31mhi".as_slice());
    sent(TerminalUpdate::Output { output }, r#"{"update":"output","output":"G1szMW1oaQ=="}"#);
    sent(TerminalUpdate::Resized { size: SIZE }, r#"{"update":"resized","size":{"cols":80,"rows":24}}"#);
    sent(TerminalUpdate::Exited(TerminalExit { code: Some(0) }), r#"{"update":"exited","code":0}"#);
    sent(TerminalUpdate::Exited(TerminalExit { code: None }), r#"{"update":"exited","code":null}"#);
}

/// Such as one a newer hub or worker sends, which a client skips rather than dropping the frame.
#[test]
fn an_update_this_build_doesnt_know_still_reads() {
    let line = br#"{"frame":"terminal","subscription_id":3,"update":{"update":"title","title":"vim"}}"#;
    let frame: HubFrame = frame::decode(line).unwrap();
    assert_eq!(frame, HubFrame::Terminal { subscription_id: 3, update: TerminalUpdate::Unknown });
}

#[test]
fn every_request_a_hub_makes_of_a_workers_terminal_keeps_its_form() {
    let env = vec![("PORT".into(), "55000".into())];
    let start =
        worker::TerminalStart { terminal: "t".into(), size: SIZE, cwd: "/w/aspen".into(), env, opened_at_ms: 5 };
    asked(
        WorkerRequest::TerminalStart(start),
        r#"{"method":"terminal_start","terminal":"t","size":{"cols":80,"rows":24},"cwd":"/w/aspen","env":[["PORT","55000"]],"opened_at_ms":5}"#,
    );
    let input = worker::TerminalInput { terminal: "t".into(), input: b"\x03".as_slice().into() };
    asked(WorkerRequest::TerminalInput(input), r#"{"method":"terminal_input","terminal":"t","input":"Aw=="}"#);
    let resize = worker::TerminalResize { terminal: "t".into(), size: SIZE };
    asked(
        WorkerRequest::TerminalResize(resize),
        r#"{"method":"terminal_resize","terminal":"t","size":{"cols":80,"rows":24}}"#,
    );
    let show = worker::TerminalShow { terminal: "t".into() };
    asked(WorkerRequest::TerminalShow(show), r#"{"method":"terminal_show","terminal":"t"}"#);
    let stop = worker::TerminalStop { terminal: "t".into() };
    asked(WorkerRequest::TerminalStop(stop), r#"{"method":"terminal_stop","terminal":"t"}"#);
    asked(WorkerRequest::RunningTerminals, r#"{"method":"running_terminals"}"#);
}

#[test]
fn every_reply_and_update_a_worker_sends_of_its_terminals_keeps_its_form() {
    let screen = TerminalScreen { size: SIZE, bytes: b"\x1b[2J\x1b[Hhi".as_slice().into() };
    answered(
        WorkerResponse::TerminalShown(TerminalShown { screen, next: UpdateIndex::new(12) }),
        r#"{"result":"terminal_shown","screen":{"size":{"cols":80,"rows":24},"bytes":"G1syShtbSGhp"},"next":12}"#,
    );
    let running = RunningTerminal { terminal: "t".into(), opened_at_ms: 5 };
    answered(
        WorkerResponse::RunningTerminals(RunningTerminalsReply { terminals: vec![running] }),
        r#"{"result":"running_terminals","terminals":[{"terminal":"t","opened_at_ms":5}]}"#,
    );

    let output = TerminalUpdate::Output { output: b"\x1b[31mhi".as_slice().into() };
    let update = WorkerFrame::Terminal { terminal: "t".into(), at: UpdateIndex::new(12), update: output };
    keeps_its_form(
        &update,
        r#"{"frame":"terminal","terminal":"t","at":12,"update":{"update":"output","output":"G1szMW1oaQ=="}}"#,
    );
}

/// So a client can say what stopping the hub would end, and a hub from before terminals has none to count.
#[test]
fn a_hubs_status_counts_its_open_terminals() {
    let fields = r#""pid":7,"hub_version":"a","protocol_version":4,"started_at_ms":1,"workers":1,"running_sessions":0,"clients":1,"on_demand":true"#;
    let now: HubStatus = serde_json::from_str(&format!(r#"{{{fields},"open_terminals":2}}"#)).unwrap();
    let before: HubStatus = serde_json::from_str(&format!("{{{fields}}}")).unwrap();

    assert_eq!(now.open_terminals, 2);
    assert!(!now.is_idle(), "stopping it would end their shells");
    assert_eq!(before.open_terminals, 0, "from a hub from before terminals");
    assert!(before.is_idle());
}
