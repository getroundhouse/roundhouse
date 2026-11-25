//! An agent reaching the hub's tool servers through its worker, which sends each the key no agent is given.

use std::net::Ipv4Addr;
use std::time::Duration;

use protocol::model::{ToolServerHeader, ToolServerName, ToolServerUrl};
use protocol::worker::{
    ProxiedServer, ServeTools, ServerHeader, ServerHeaderValue, ToolServersSet, ToolsToken, WorkerOutcome,
    WorkerRequest, WorkerResponse,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use super::{KEEPS_ITS_MCP_CONFIG, Worker, agent_start, cli, config, given, hello, reply, send, start_worker};

/// Answers one call as an MCP server would, and gives back what it was asked.
async fn one_call(server: TcpListener) -> String {
    let (stream, _) = server.accept().await.unwrap();
    let mut stream = BufReader::new(stream);
    let mut asked = String::new();
    while !asked.ends_with("\r\n\r\n") {
        stream.read_line(&mut asked).await.unwrap();
    }
    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
    let answer = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n", body.len());
    stream.write_all(format!("{answer}{body}").as_bytes()).await.unwrap();
    asked
}

/// The set holds for an agent whose start was sent right behind it, and the worker keeps what it was set in no file.
#[tokio::test]
async fn an_agent_is_launched_with_the_servers_set_before_it_and_reaches_one_with_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let set = ProxiedServer {
        id: "t-1".into(),
        name: "linear".into(),
        label: ToolServerName::typed("Linear").unwrap(),
        url: ToolServerUrl::typed(&format!("http://{}/mcp", server.local_addr().unwrap())).unwrap(),
        header: Some(ServerHeader {
            name: ToolServerHeader::typed("X-Key").unwrap(),
            value: ServerHeaderValue::new("made-up-key"),
        }),
        sign_in: None,
    };
    let Worker { read: hub_read, write: mut hub_write, running } =
        start_worker(config(dir.path(), Some(cli(dir.path(), KEEPS_ITS_MCP_CONFIG))));
    let mut frames = BufReader::new(hub_read).lines();
    send(&mut hub_write, [hello(), WorkerRequest::ServeTools(ServeTools { port: None })]).await;
    reply(&mut frames, 1).await;
    let calls = [
        WorkerRequest::ToolServersSet(ToolServersSet { servers: vec![set] }),
        WorkerRequest::AgentStart(agent_start("s", dir.path(), Some(ToolsToken::new("token-s")))),
    ];
    send(&mut hub_write, calls).await;
    let applied = reply(&mut frames, 0).await;
    let told: Value = serde_json::from_str(&given(dir.path()).await.1).unwrap();
    let url = told["mcpServers"]["linear"]["url"].as_str().unwrap().strip_prefix("http://").unwrap().to_string();
    let authorization = told["mcpServers"]["linear"]["headers"]["Authorization"].as_str().unwrap();
    let (at, path) = url.split_once('/').unwrap();

    let asked = tokio::spawn(one_call(server));
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let mut stream = TcpStream::connect(at).await.unwrap();
    let call =
        format!("POST /{path} HTTP/1.1\r\nHost: {at}\r\nAuthorization: {authorization}\r\nConnection: close\r\n");
    stream.write_all(format!("{call}Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    let mut answer = String::new();
    timeout(Duration::from_secs(10), stream.read_to_string(&mut answer)).await.expect("an answer").unwrap();

    assert_eq!(applied, WorkerOutcome::Response(WorkerResponse::Ok));
    assert!(answer.starts_with("HTTP/1.1 200") && answer.contains(r#""tools":[]"#), "{answer}");
    let asked = asked.await.unwrap().to_ascii_lowercase();
    assert!(asked.contains("x-key: made-up-key\r\n") && !asked.contains("token-s"), "{asked}");
    for kept in files_in(dir.path()) {
        let kept = std::fs::read(kept).unwrap();
        assert!(!String::from_utf8_lossy(&kept).contains("made-up-key"));
    }
    hub_write.shutdown().await.unwrap();
    timeout(Duration::from_secs(10), running).await.expect("the worker didn't exit").unwrap();
}

/// Every file under `dir`, which is all a worker in it writes to.
fn files_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
        if entry.file_type().unwrap().is_dir() {
            files.extend(files_in(&entry.path()));
        } else {
            files.push(entry.path());
        }
    }
    assert!(!files.is_empty());
    files
}
