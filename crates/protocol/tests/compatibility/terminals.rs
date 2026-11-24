//! Every message about a terminal, spelled out, since a client, its hub and the hub's workers can each be a different
//! build, and a worker outlives the hub that started it.

use protocol::client::terminals::{TerminalClose, TerminalOpen, TerminalRows};
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, HubNotice, HubStatus, ReplyOutcome};
use protocol::frame;
use protocol::model::{RecordKey, Records, Rev, Terminal};
use protocol::terminal::{
    CellColor, CellStyle, Numbering, RowJson, RowNumber, RowRun, Span, TerminalExit, TerminalPicture, TerminalRow,
    TerminalRowsReply, TerminalSize, TerminalUpdate,
};
use protocol::worker::{self, RunningTerminal, RunningTerminalsReply, WorkerFrame, WorkerRequest, WorkerResponse};

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
    let rows = TerminalRows { id: "t".into(), numbering: Numbering::new(2), from: RowNumber::new(40), count: 100 };
    client_asked(
        ClientRequest::TerminalRows(rows),
        r#"{"method":"terminal_rows","id":"t","numbering":2,"from":40,"count":100}"#,
    );
    client_asked(
        ClientRequest::TerminalClose(TerminalClose { id: "t".into() }),
        r#"{"method":"terminal_close","id":"t"}"#,
    );
}

/// The requests of the way before rows, from an app that sent a terminal's bytes, are ones a hub no longer knows, so
/// it refuses each as `Unsupported` and carries on.
#[test]
fn a_request_from_before_rows_is_one_a_hub_doesnt_know() {
    let before = [
        r#"{"method":"terminal_attach","id":"t","subscription_id":3}"#,
        r#"{"method":"terminal_input","id":"t","input":"bHMN"}"#,
        r#"{"method":"terminal_resize","id":"t","size":{"cols":120,"rows":40}}"#,
    ];
    for request in before {
        let read = frame::decode::<ClientRequest>(request.as_bytes());
        assert!(matches!(read, Ok(ClientRequest::Unknown)), "{request} read as {read:?}");
    }
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

/// What a hub sends a client that watches: a frame of a picture, a row in it as the worker wrote it, inside a frame
/// whose tag serde reads first, and the rows a client asks for.
#[test]
fn what_a_hub_sends_a_client_that_watches_keeps_its_form() {
    let span = Span { text: "hi".into(), style: Some(0), ..Span::default() };
    let style = CellStyle { foreground: Some(CellColor::Palette(2)), ..CellStyle::default() };
    let row = TerminalRow { spans: vec![span], styles: vec![style], wraps: true };
    let picture = TerminalPicture {
        numbering: Numbering::new(3),
        size: SIZE,
        top: RowNumber::new(900),
        first: RowNumber::new(30),
        rows: vec![RowRun { from: RowNumber::new(901), rows: vec![RowJson::new(&row).unwrap()] }],
        kept_from: RowNumber::new(800),
        whole: false,
        fresh: false,
        cursor: None,
        state: None,
    };
    keeps_its_form(
        &HubFrame::TerminalPicture { subscription_id: 4, picture },
        concat!(
            r#"{"frame":"terminal_picture","subscription_id":4,"picture":{"numbering":3,"size":{"cols":80,"rows":24},"#,
            r#""top":900,"first":30,"rows":[{"from":901,"rows":[{"s":[{"t":"hi","y":0}],"w":true,"y":[{"f":2}]}]}],"#,
            r#""kept_from":800}}"#,
        ),
    );

    let rows = TerminalRowsReply {
        numbering: Numbering::new(3),
        first: RowNumber::new(30),
        top: RowNumber::new(900),
        from: RowNumber::new(40),
        rows: vec![row],
    };
    keeps_its_form(
        &HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(ClientResponse::TerminalRows(rows)) },
        concat!(
            r#"{"frame":"reply","id":7,"response":{"result":"terminal_rows","numbering":3,"first":30,"top":900,"#,
            r#""from":40,"rows":[{"s":[{"t":"hi","y":0}],"w":true,"y":[{"f":2}]}]}}"#,
        ),
    );
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
    let rows = worker::TerminalRows {
        terminal: "t".into(),
        numbering: Numbering::new(2),
        from: RowNumber::new(40),
        count: 100,
    };
    asked(
        WorkerRequest::TerminalRows(rows),
        r#"{"method":"terminal_rows","terminal":"t","numbering":2,"from":40,"count":100}"#,
    );
    let watch = worker::TerminalWatch { terminal: "t".into(), watching: true };
    asked(WorkerRequest::TerminalWatch(watch), r#"{"method":"terminal_watch","terminal":"t","watching":true}"#);
    let stop = worker::TerminalStop { terminal: "t".into() };
    asked(WorkerRequest::TerminalStop(stop), r#"{"method":"terminal_stop","terminal":"t"}"#);
    asked(WorkerRequest::RunningTerminals, r#"{"method":"running_terminals"}"#);
}

#[test]
fn every_reply_and_update_a_worker_sends_of_its_terminals_keeps_its_form() {
    let row = TerminalRow { spans: vec![Span { text: "hi".into(), ..Span::default() }], ..TerminalRow::default() };
    let rows = TerminalRowsReply {
        numbering: Numbering::new(2),
        first: RowNumber::new(30),
        top: RowNumber::new(900),
        from: RowNumber::new(40),
        rows: vec![row, TerminalRow::default()],
    };
    answered(
        WorkerResponse::TerminalRows(rows),
        r#"{"result":"terminal_rows","numbering":2,"first":30,"top":900,"from":40,"rows":[{"s":[{"t":"hi"}]},{}]}"#,
    );
    let running = RunningTerminal { terminal: "t".into(), opened_at_ms: 5 };
    answered(
        WorkerResponse::RunningTerminals(RunningTerminalsReply { terminals: vec![running] }),
        r#"{"result":"running_terminals","terminals":[{"terminal":"t","opened_at_ms":5}]}"#,
    );

    let ended = TerminalUpdate::Exited(TerminalExit { code: Some(3) });
    let update = WorkerFrame::Terminal { terminal: "t".into(), update: ended };
    keeps_its_form(&update, r#"{"frame":"terminal","terminal":"t","update":{"update":"exited","code":3}}"#);
}

/// A worker from before rows goes on sending what its programs write, numbered, which a hub skips, and its shells'
/// ends, which a hub still reads.
#[test]
fn what_a_worker_from_before_rows_sends_of_a_terminal_still_reads() {
    let wrote = br#"{"frame":"terminal","terminal":"t","at":12,"update":{"update":"output","output":"G1szMW1oaQ=="}}"#;
    let resized =
        br#"{"frame":"terminal","terminal":"t","at":13,"update":{"update":"resized","size":{"cols":80,"rows":24}}}"#;
    let ended = br#"{"frame":"terminal","terminal":"t","at":14,"update":{"update":"exited","code":0}}"#;

    let read = |line: &[u8]| frame::decode::<WorkerFrame>(line).unwrap();

    let skipped = WorkerFrame::Terminal { terminal: "t".into(), update: TerminalUpdate::Unknown };
    assert_eq!(read(wrote), skipped);
    assert_eq!(read(resized), skipped);
    let exited = TerminalUpdate::Exited(TerminalExit { code: Some(0) });
    assert_eq!(read(ended), WorkerFrame::Terminal { terminal: "t".into(), update: exited });
}

/// The requests of the way before rows, from a hub that followed a terminal's bytes, are ones a worker no longer
/// knows, so it refuses each as `Unsupported` and carries on.
#[test]
fn a_request_from_before_rows_is_one_a_worker_doesnt_know() {
    let before = [
        r#"{"method":"terminal_input","terminal":"t","input":"Aw=="}"#,
        r#"{"method":"terminal_resize","terminal":"t","size":{"cols":80,"rows":24}}"#,
        r#"{"method":"terminal_show","terminal":"t"}"#,
    ];
    for request in before {
        let read = frame::decode::<WorkerRequest>(request.as_bytes());
        assert!(matches!(read, Ok(WorkerRequest::Unknown)), "{request} read as {read:?}");
    }
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
