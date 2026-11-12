//! The worker's main loop, driven directly over an in-memory stream.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::frame;
use protocol::model::{AgentKind, RunId, SessionSettings};
use protocol::worker::{
    AgentSpec, AskAnswer, AskOutcome, AskReply, ToolsToken, WORKER_PROTOCOL_VERSION, WorkerAsk, WorkerCall,
    WorkerFrame, WorkerOutcome, WorkerRequest, WorkerResponse,
};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use worker::{Config, Deleter, serve};

/// What a hub of this build says first.
fn hello() -> WorkerRequest {
    WorkerRequest::Hello {
        protocol_version: WORKER_PROTOCOL_VERSION,
        oldest_protocol_version: Some(WORKER_PROTOCOL_VERSION),
    }
}

/// Neither an agent's start nor serving its tools, under way as the hub goes, can leave anything behind that keeps the
/// worker running.
#[tokio::test]
async fn a_start_racing_the_hub_going_away_cannot_leave_the_worker_running() {
    let dir = tempfile::tempdir().unwrap();
    let cli = waiting_cli(dir.path());

    for attempt in 0..8 {
        let Worker { read: mut hub_read, write: mut hub_write, running } =
            start_worker(config(dir.path(), Some(cli.clone())));
        tokio::spawn(async move {
            let mut sink = Vec::new();
            let _ = hub_read.read_to_end(&mut sink).await;
        });

        let calls = [
            hello(),
            WorkerRequest::ServeTools { port: None },
            WorkerRequest::AgentStart(agent_start(&format!("s{attempt}"), dir.path(), None)),
        ];
        send(&mut hub_write, calls).await;
        // Goes away with the start still in flight.
        hub_write.shutdown().await.unwrap();
        drop(hub_write);

        timeout(Duration::from_secs(10), running)
            .await
            .unwrap_or_else(|_| panic!("attempt {attempt}: the worker didn't exit"))
            .unwrap();
    }
}

/// Each request runs on a task of its own, so the stop could be handled before the setup it follows. On
/// the same runtime as the worker, which tends to run the task it spawned last first.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_right_behind_a_setup_stops_it_even_before_its_script_starts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".conductor")).unwrap();
    std::fs::write(dir.path().join(".conductor/settings.toml"), "[scripts]\nsetup = \"sleep 60\"\n").unwrap();
    let Worker { read: hub_read, write: mut hub_write, .. } = start_worker(config(dir.path(), None));
    let path = dir.path().to_string_lossy().into_owned();
    let calls = [
        hello(),
        WorkerRequest::SetupWorkspace { repo_path: path.clone(), path, env: vec![] },
        WorkerRequest::CancelSetup,
    ];
    let mut lines = Vec::new();
    for (id, request) in calls.into_iter().enumerate() {
        lines.extend(frame::encode(&WorkerCall { id: id as u64, request }).unwrap());
    }

    // In one write, so the worker reads the stop before the setup has done anything.
    hub_write.write_all(&lines).await.unwrap();

    let mut replies = BufReader::new(hub_read).lines();
    let setup = timeout(Duration::from_secs(20), async {
        loop {
            let line = replies.next_line().await.unwrap().expect("a reply to the setup");
            if let WorkerFrame::Reply { id: 1, outcome } = frame::decode(line.as_bytes()).unwrap() {
                return outcome;
            }
        }
    })
    .await
    .expect("the script should have been stopped");
    assert_eq!(setup, WorkerOutcome::Response(WorkerResponse::SetupDone { code: None }));
}

/// From a newer hub: one it doesn't know, and a known one with a value it can't read. Either is refused, so the hub
/// does without it, rather than left for the hub's call to wait on until the connection closes.
#[tokio::test]
async fn a_request_from_a_newer_hub_is_refused_and_the_worker_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let calls = [
        frame::encode(&WorkerCall { id: 0, request: hello() }).unwrap(),
        b"{\"id\":1,\"request\":{\"method\":\"from_a_newer_hub\",\"path\":\"a\"}}\n".to_vec(),
        b"{\"id\":2,\"request\":{\"method\":\"account_for\",\"agent\":{\"newer\":true}}}\n".to_vec(),
        frame::encode(&WorkerCall { id: 3, request: hello() }).unwrap(),
    ];

    let answered = replies_to(config(dir.path(), None), &calls).await;

    for id in [1, 2] {
        assert!(matches!(&answered[&id], WorkerOutcome::Error(e) if e.code == ErrorCode::Unsupported), "{answered:?}");
    }
    assert!(matches!(answered[&3], WorkerOutcome::Response(WorkerResponse::Hello { .. })), "it carried on");
}

/// A worker refused its instance answers every request by saying so, one it can't read included, so the hub can tell
/// it from a worker that failed to start.
#[tokio::test]
async fn a_refused_worker_says_so_even_to_a_request_it_cant_read() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config { refused: Some("another worker is serving it".into()), ..config(dir.path(), None) };
    let calls = [b"{\"id\":1,\"request\":{\"method\":\"account_for\",\"agent\":{\"newer\":true}}}\n".to_vec()];

    let answered = replies_to(config, &calls).await;

    assert!(matches!(&answered[&1], WorkerOutcome::Error(e) if e.code == ErrorCode::Conflict), "{answered:?}");
}

/// A call to the worker's tools from an agent it started, with what its CLI was told of them, in a file of the run's own
/// in the worker's state directory, is asked of the hub, for that agent's chat, and answered with what the hub says.
#[tokio::test]
async fn an_agents_call_to_the_tools_is_asked_of_the_hub_and_answered_with_its_answer() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let cli = cli(dir.path(), KEEPS_ITS_MCP_CONFIG);
    let Worker { read: hub_read, write: mut hub_write, running } =
        start_worker(Config { state_dir: Some(state.clone()), ..config(dir.path(), Some(cli)) });
    let mut frames = BufReader::new(hub_read).lines();
    let calls = [
        hello(),
        WorkerRequest::ServeTools { port: None },
        WorkerRequest::AgentStart(agent_start("s", dir.path(), Some(ToolsToken::new("token-s")))),
    ];
    send(&mut hub_write, calls).await;
    let WorkerOutcome::Response(WorkerResponse::ToolsServing { port }) = reply(&mut frames, 1).await else {
        panic!("the tools aren't served");
    };
    reply(&mut frames, 2).await;
    let (path, told) = given(dir.path()).await;
    assert_eq!(Path::new(&path), state.join("agents/s-1.mcp.json"));
    let told: serde_json::Value = serde_json::from_str(&told).unwrap();
    let server = &told["mcpServers"]["roundhouse"];
    assert_eq!(server["url"], format!("http://127.0.0.1:{port}/mcp"));
    let token = server["headers"]["Authorization"].as_str().unwrap().strip_prefix("Bearer ").unwrap().to_string();

    let calling = tokio::spawn(async move { call_tool(port, &token).await });
    let (id, session_id, ask) = timeout(Duration::from_secs(10), async {
        loop {
            if let WorkerFrame::Ask { id, session_id, ask } = next_frame(&mut frames).await {
                return (id, session_id, ask);
            }
        }
    })
    .await
    .expect("the hub was never asked");
    let answer = AskAnswer { answer: id, outcome: AskOutcome::Response(AskReply::PullRequest { pull_request: None }) };
    hub_write.write_all(&frame::encode(&answer).unwrap()).await.unwrap();
    let response = timeout(Duration::from_secs(10), calling).await.expect("the call was never answered").unwrap();

    assert_eq!((session_id.as_str(), ask), ("s", WorkerAsk::PullRequest));
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("no pull request on GitHub"), "{response}");
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker didn't exit").unwrap();
}

/// A worker serving over an in-memory stream, and the hub's ends of it.
struct Worker {
    read: ReadHalf<DuplexStream>,
    write: WriteHalf<DuplexStream>,
    running: JoinHandle<()>,
}

fn start_worker(config: Config) -> Worker {
    let (hub_end, worker_end) = tokio::io::duplex(64 * 1024);
    let (worker_read, worker_write) = tokio::io::split(worker_end);
    let (read, write) = tokio::io::split(hub_end);
    Worker { read, write, running: tokio::spawn(serve(worker_read, worker_write, config)) }
}

/// A worker's config for workspaces in `dir`, whose Claude CLI is `claude_bin`.
fn config(dir: &Path, claude_bin: Option<PathBuf>) -> Config {
    Config {
        workspaces_root: dir.to_path_buf(),
        state_dir: None,
        home: None,
        path: None,
        claude_bin,
        scratch_dir: dir.to_path_buf(),
        deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
        refused: None,
    }
}

/// Serves a worker running with `config`, sends it `calls` in one go, and collects its reply to each, by ID.
async fn replies_to(config: Config, calls: &[Vec<u8>]) -> std::collections::BTreeMap<u64, WorkerOutcome> {
    let Worker { read: hub_read, write: mut hub_write, .. } = start_worker(config);
    for call in calls {
        hub_write.write_all(call).await.unwrap();
    }
    let mut replies = BufReader::new(hub_read).lines();
    let mut answered = std::collections::BTreeMap::new();
    while answered.len() < calls.len() {
        let line = timeout(Duration::from_secs(10), replies.next_line()).await.expect("a reply").unwrap().unwrap();
        if let WorkerFrame::Reply { id, outcome } = frame::decode(line.as_bytes()).unwrap() {
            answered.insert(id, outcome);
        }
    }
    answered
}

/// A hub speaks the version before its own too, for 30 days after a break, and a worker takes one whose hello names
/// its own version, but not one that no longer speaks it.
#[tokio::test]
async fn a_worker_takes_a_hub_whose_hello_names_its_version_in_a_range() {
    let dir = tempfile::tempdir().unwrap();
    let ours = WORKER_PROTOCOL_VERSION;
    let hellos = [
        WorkerRequest::Hello { protocol_version: ours + 1, oldest_protocol_version: Some(ours) },
        WorkerRequest::Hello { protocol_version: ours + 2, oldest_protocol_version: Some(ours + 1) },
        WorkerRequest::Hello { protocol_version: ours, oldest_protocol_version: None },
    ];
    let calls: Vec<_> = hellos
        .into_iter()
        .enumerate()
        .map(|(id, request)| frame::encode(&WorkerCall { id: id as u64, request }).unwrap())
        .collect();

    let answered = replies_to(config(dir.path(), None), &calls).await;

    let chosen = WorkerOutcome::Response(WorkerResponse::Hello { protocol_version: ours });
    assert_eq!(answered[&0], chosen, "a newer hub that still speaks this worker's version");
    assert!(matches!(&answered[&1], WorkerOutcome::Error(e) if e.code == ErrorCode::VersionMismatch), "{answered:?}");
    assert_eq!(answered[&2], chosen, "a hub from before ranges");
}

/// A stand-in for the Claude CLI, in `dir`, that runs `body`.
fn cli(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let cli = dir.join("fake-cli");
    std::fs::write(&cli, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli
}

/// A stand-in for the Claude CLI, in `dir`, that only waits.
fn waiting_cli(dir: &Path) -> PathBuf {
    cli(dir, "exec sleep 100")
}

/// Writes the path it's given as `--mcp-config` to `given`, in the directory it runs in, with what the file says after
/// it, or an empty line if it's given none, then waits.
const KEEPS_ITS_MCP_CONFIG: &str = r#"while [ $# -gt 0 ] && [ "$1" != --mcp-config ]; do shift; done
{ printf '%s\n' "$2"; [ $# -gt 0 ] && cat "$2"; } > given.tmp; mv given.tmp given; exec sleep 100"#;

/// The path a CLI running [`KEEPS_ITS_MCP_CONFIG`] in `dir` was given, and what the file said, once it has written them.
async fn given(dir: &Path) -> (String, String) {
    let given = dir.join("given");
    timeout(Duration::from_secs(10), async {
        while !given.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the CLI to start");
    let given = std::fs::read_to_string(given).unwrap();
    let (path, told) = given.split_once('\n').unwrap();
    (path.to_string(), told.to_string())
}

/// What the agent's CLI is given as `--mcp-config`, as a worker with `config` starts it with a token, after being asked
/// to serve its tools if `serving`.
async fn told_at_start(dir: &Path, config: Config, serving: bool) -> String {
    let Worker { read: hub_read, write: mut hub_write, running } = start_worker(config);
    let mut frames = BufReader::new(hub_read).lines();
    let mut calls = vec![hello()];
    calls.extend(serving.then_some(WorkerRequest::ServeTools { port: None }));
    calls.push(WorkerRequest::AgentStart(agent_start("s", dir, Some(ToolsToken::new("token-s")))));
    let last = calls.len() as u64 - 1;
    send(&mut hub_write, calls).await;
    let started = reply(&mut frames, last).await;
    assert!(matches!(started, WorkerOutcome::Response(WorkerResponse::AgentStarted { .. })), "{started:?}");
    let (path, _) = given(dir).await;
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker didn't exit").unwrap();
    path
}

fn agent_start(session_id: &str, cwd: &Path, tools_token: Option<ToolsToken>) -> AgentSpec {
    AgentSpec {
        session_id: session_id.into(),
        run: RunId::new(1),
        agent: AgentKind::Claude,
        cwd: cwd.to_string_lossy().into_owned(),
        agent_session_id: "a".into(),
        resume: false,
        settings: SessionSettings::default(),
        env: vec![],
        tools_token,
    }
}

/// Sends each request as a call, numbered from 0.
async fn send(hub_write: &mut WriteHalf<DuplexStream>, calls: impl IntoIterator<Item = WorkerRequest>) {
    for (id, request) in calls.into_iter().enumerate() {
        hub_write.write_all(&frame::encode(&WorkerCall { id: id as u64, request }).unwrap()).await.unwrap();
    }
}

async fn next_frame<R: AsyncRead + Unpin>(frames: &mut Lines<BufReader<R>>) -> WorkerFrame {
    let line = frames.next_line().await.unwrap().expect("the worker went on");
    frame::decode(line.as_bytes()).unwrap()
}

async fn reply<R: AsyncRead + Unpin>(frames: &mut Lines<BufReader<R>>, to: u64) -> WorkerOutcome {
    timeout(Duration::from_secs(10), async {
        loop {
            if let WorkerFrame::Reply { id, outcome } = next_frame(frames).await
                && id == to
            {
                return outcome;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("call {to} was never answered"))
}

/// Calls `get_pull_request` as the Claude CLI does, over one connection, and returns the whole response. The fake agent,
/// in `apps/daemon/examples/fake_claude/mcp.rs`, writes the same request, so a change to what the CLI sends goes in both.
async fn call_tool(port: u16, token: &str) -> String {
    let meta = r#"{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}"#;
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"get_pull_request","arguments":{{}},"_meta":{meta}}}}}"#
    );
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nMCP-Protocol-Version: 2026-07-28\r\nMcp-Method: tools/call\r\n\
         Mcp-Name: get_pull_request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn an_agent_goes_without_the_tools_when_they_arent_served_or_there_is_nowhere_to_tell_its_cli() {
    let (unserved, untold) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let with_state =
        |dir: &Path| Config { state_dir: Some(dir.join("state")), ..config(dir, Some(cli(dir, KEEPS_ITS_MCP_CONFIG))) };

    let unserved = told_at_start(unserved.path(), with_state(unserved.path()), false).await;
    let untold =
        told_at_start(untold.path(), config(untold.path(), Some(cli(untold.path(), KEEPS_ITS_MCP_CONFIG))), true).await;

    assert_eq!((unserved.as_str(), untold.as_str()), ("", ""), "started without a --mcp-config");
}
