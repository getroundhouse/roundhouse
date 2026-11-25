//! The hub's tool servers, which an agent calls through its worker, here a stand-in for one in the test's process.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use client_core::Client;
use protocol::client::tool_servers::{ToolServerAdd, ToolServerRemove};
use protocol::model::{ToolServerAuthGiven, ToolServerKey};

use crate::support::{Stack, calls, read_all, wait_until};

const KEY: &str = "made-up-key-of-a-person";

const CALL: &str = "mcp:Linear_Northwind/find_issue {}";

/// An MCP server with one tool, which refuses a request without [`KEY`], and says the method of each with it.
/// It's there until the test's process exits.
fn stand_in() -> (String, Receiver<String>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let (methods, heard) = channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let methods = methods.clone();
            std::thread::spawn(move || serve(&stream.unwrap(), &methods));
        }
    });
    (url, heard)
}

/// Answers one connection's one request, and closes it.
fn serve(mut stream: &TcpStream, methods: &Sender<String>) {
    let mut from = BufReader::new(stream);
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        if from.read_line(&mut line).unwrap_or_default() == 0 {
            return;
        }
        let line = line.trim_end().to_string();
        if line.is_empty() {
            break;
        }
        lines.push(line);
    }
    let field = |name: &str| {
        let fields = lines.iter().filter_map(|line| line.split_once(": "));
        fields.filter(|(given, _)| given.eq_ignore_ascii_case(name)).map(|(_, value)| value.to_string()).next()
    };
    let mut body = vec![0; field("content-length").unwrap().parse().unwrap()];
    from.read_exact(&mut body).unwrap();
    let asked: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let (status, result) = if field("x-key").as_deref() == Some(KEY) {
        let _ = methods.send(asked["method"].as_str().unwrap().to_string());
        let result = match asked["method"].as_str() {
            Some("tools/list") => serde_json::json!({ "tools": [{ "name": "find_issue" }] }),
            Some("tools/call") => serde_json::json!({ "content": [{ "type": "text", "text": "Found issue 7" }] }),
            _ => serde_json::json!({ "capabilities": {} }),
        };
        ("200 OK", result)
    } else {
        ("401 Unauthorized", serde_json::Value::Null)
    };
    let body = serde_json::json!({ "jsonrpc": "2.0", "id": asked["id"], "result": result }).to_string();
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}", body.len());
    stream.write_all(format!("{head}\r\nConnection: close\r\n\r\n{body}").as_bytes()).unwrap();
}

/// Every file under `dir` that holds `text`, such as the hub's database, its log and the agent's copy of what it was
/// launched with.
fn holding(dir: &Path, text: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            found.extend(holding(&entry.path(), text));
        } else if kind.is_file()
            && let Ok(bytes) = std::fs::read(entry.path())
            && bytes.windows(text.len()).any(|bytes| bytes == text.as_bytes())
        {
            found.push(entry.path());
        }
    }
    found
}

/// Adds the server at `url` as `Linear (Northwind)`, which agents call `Linear_Northwind`, with [`KEY`].
fn add(client: &Client, url: String) {
    let auth = ToolServerAuthGiven::Key { header: "X-Key".into(), key: ToolServerKey::new(KEY) };
    client.call(ToolServerAdd { id: "t".into(), name: "Linear (Northwind)".into(), url, auth }).unwrap();
}

#[test]
fn an_agent_calls_a_server_the_person_added_with_their_key_which_neither_it_nor_the_transcript_has() {
    let (url, heard) = stand_in();
    let mut stack = Stack::start();
    let client = stack.client();
    add(&client, url);
    let (_, chat) = stack.workspace(&client);

    let answered = calls::run(&client, &chat.id, CALL, 1);

    assert_eq!(answered, [("Found issue 7".to_string(), false)]);
    assert_eq!(heard.try_iter().last().as_deref(), Some("tools/call"), "it took the key from the worker");
    let kept = stack.agent_state().join("last-mcp-config");
    assert!(std::fs::read_to_string(kept).unwrap().contains("Linear_Northwind"), "what's searched below");
    assert!(!format!("{:?}", read_all(&client, &chat.id)).contains(KEY));
    drop(client);
    stack.stop_hub();
    let holding = holding(&stack.path(""), KEY);
    let [file] = holding.as_slice() else { panic!("not only the hub's own file: {holding:?}") };
    assert_eq!(file.parent(), Some(stack.path("home/tool-servers").as_path()));
}

/// Its worker is set the rest while it runs, and the agent that was given the server is told why its calls fail.
#[test]
fn an_agent_that_has_a_server_the_person_removes_is_told_it_was_removed() {
    let (url, _) = stand_in();
    let stack = Stack::start();
    let client = stack.client();
    add(&client, url);
    let (_, chat) = stack.workspace(&client);
    assert_eq!(calls::run(&client, &chat.id, CALL, 1), [("Found issue 7".to_string(), false)]);

    client.call(ToolServerRemove { id: "t".into() }).unwrap();

    let (mut turn, mut answered) = (1, Vec::new());
    wait_until("a call to fail", || {
        turn += 1;
        answered = calls::run(&client, &chat.id, CALL, turn);
        answered.iter().any(|(_, is_error)| *is_error)
    });
    assert!(answered[0].0.contains("was removed"), "{answered:?}");
}
