//! The hub's tool servers, which an agent calls through its worker, here a stand-in for one in the test's process.

#![cfg(test)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use client_core::{Client, RecordsFeed};
use hub::{
    ToolServerPage, ToolServerRefreshing, ToolServerRegisters, ToolServerSaysAllowed, ToolServerScript,
    ToolServerStandIn, ToolServerWants,
};
use protocol::capability::settings::Settings;
use protocol::capability::setup::ServerState;
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit, ToolServerRemove};
use protocol::client::tool_sign_ins::{ToolSignInBegun, ToolSignInReturned, ToolSignInTo, redirect};
use protocol::event::EventBody;
use protocol::model::{
    AgentKind, Observation, Observed, SessionId, ToolServerAuth, ToolServerAuthGiven, ToolServerKey, ToolSignInStatus,
};

use crate::carrying_on::update;
use crate::support::{Stack, calls, read_all, session_records, wait_until};

pub(crate) const KEY: &str = "made-up-key-of-a-person";

/// Another of theirs.
pub(crate) const ANOTHER: &str = "made-up-other-key-of-a-person";

pub(crate) const CALL: &str = "mcp:Linear_Northwind/find_issue {}";

/// A request a stand-in took: its method, the key it came with, and its path, with its query.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Took {
    pub(crate) method: String,
    pub(crate) key: String,
    pub(crate) path: String,
}

/// An MCP server with one tool, which refuses a request without [`KEY`] or [`ANOTHER`], and says each with one. It's
/// there until the test's process exits.
pub(crate) fn stand_in() -> (String, Receiver<Took>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let (took, heard) = channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let took = took.clone();
            std::thread::spawn(move || serve(&stream.unwrap(), &took));
        }
    });
    (url, heard)
}

/// Answers one connection's one request, and closes it.
pub(crate) fn serve(mut stream: &TcpStream, took: &Sender<Took>) {
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
    let (status, result) = if let Some(key) = field("x-key").filter(|key| [KEY, ANOTHER].contains(&key.as_str())) {
        let path = lines[0].split(' ').nth(1).unwrap().to_string();
        let _ = took.send(Took { method: asked["method"].as_str().unwrap().to_string(), key, path });
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
pub(crate) fn holding(dir: &Path, text: &str) -> Vec<PathBuf> {
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
pub(crate) fn add(client: &Client, url: String) {
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
    let took = Took { method: "tools/call".into(), key: KEY.into(), path: "/mcp".into() };
    assert_eq!(heard.try_iter().last(), Some(took), "it took the key from the worker");
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

/// The server at the stand-in, as `Linear (Northwind)`.
fn new(stand_in: &ToolServerStandIn) -> ToolSignInTo {
    let url = stand_in.server().as_str().to_string();
    ToolSignInTo::New { id: "t".into(), name: "Linear (Northwind)".into(), url }
}

/// Signs in `to` the stand-in's server, as the person does in their browser, and gives the page they did on.
fn sign_in(client: &Client, stand_in: &ToolServerStandIn, to: ToolSignInTo) -> String {
    let id = client_core::new_tool_sign_in_id();
    let begun = client.tool_sign_in_start(&id, to, redirect(17867)).unwrap();
    let ToolSignInBegun::Page { page } = begun else { panic!("{begun:?}") };
    let returned = stand_in.visit(&ToolServerPage::new(page.as_str()));
    client.tool_sign_in_finish(&id, ToolSignInReturned::new(returned)).unwrap();
    page.as_str().to_string()
}

/// Its server takes no token from before, as once each has ended, which no check had renewed.
#[test]
fn an_agents_call_to_a_server_the_person_signed_in_to_goes_on_working_across_its_tokens_end() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let stand_in = runtime.block_on(ToolServerStandIn::with(ToolServerScript::default()));
    let stack = Stack::start();
    let client = stack.client();
    sign_in(&client, &stand_in, new(&stand_in));
    let (_, chat) = stack.workspace(&client);
    let works = [("Found issue 7".to_string(), false)];

    assert_eq!(calls::run(&client, &chat.id, CALL, 1), works);
    stand_in.end_access_tokens();
    assert_eq!(calls::run(&client, &chat.id, CALL, 2), works);

    assert_eq!(stand_in.asked_for("POST /token").len(), 2, "a code, and one renewal");
}

/// Its service takes no client document, so the hub registers with it, and renews as it was registered.
#[test]
fn a_server_whose_service_has_apps_register_is_signed_in_to_and_renewed() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let registers = ToolServerRegisters::Anyone;
    let script = ToolServerScript { client_documents: false, registers, ..ToolServerScript::default() };
    let stand_in = runtime.block_on(ToolServerStandIn::with(script));
    let stack = Stack::start();
    let client = stack.client();
    sign_in(&client, &stand_in, new(&stand_in));
    let (_, chat) = stack.workspace(&client);
    let works = [("Found issue 7".to_string(), false)];

    assert_eq!(calls::run(&client, &chat.id, CALL, 1), works);
    stand_in.end_access_tokens();
    assert_eq!(calls::run(&client, &chat.id, CALL, 2), works);

    let ids: Vec<_> = stand_in.asked_for("POST /token").iter().map(|asked| asked.form("client_id").unwrap()).collect();
    assert_eq!(ids, ["made-up-client-1", "made-up-client-1"], "a code, and one renewal");
    assert_eq!(stand_in.asked_for("POST /register").len(), 1);
    assert_eq!(holding(&stack.path(""), "made-up-secret"), Vec::<PathBuf>::new());
    assert_eq!(holding(&stack.path(""), "made-up-registration-token"), Vec::<PathBuf>::new());
}

/// Its other tool works before and after, for which it wants what it was first asked for.
#[test]
fn a_tool_that_wants_more_access_fails_alone_and_works_once_the_person_signs_in_again() {
    let [first, again] = signed_in_again_for_more_access(ToolServerScript::default());

    assert!(first.ends_with("&scope=read+write"), "{first}");
    assert!(again.ends_with("&scope=read+write+admin"), "{again}");
}

/// Its first request names no scope, so its service grants its own default, and says so, which signing in again would
/// lose if it asked only for what's wanted.
#[test]
fn a_tool_that_wants_more_access_than_its_services_default_gets_it_and_the_default_with_it() {
    let (allows, says_allowed) = (Some("read write"), ToolServerSaysAllowed::WhatItAllows);
    let script = ToolServerScript { challenge_scope: None, allows, says_allowed, ..ToolServerScript::default() };

    let [first, again] = signed_in_again_for_more_access(script);

    assert!(!first.contains("scope="), "{first}");
    assert!(again.ends_with("&scope=read+write+admin"), "{again}");
}

/// Has an agent call both tools of a server that answers as `script` says, one of which wants `admin` besides, before
/// and after the person signs in again. Gives each sign-in's page.
fn signed_in_again_for_more_access(script: ToolServerScript) -> [String; 2] {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let wants = |tool, scope| ToolServerWants { tool, scope };
    let wants = vec![wants("find_issue", "read"), wants("close_issue", "admin")];
    let stand_in = runtime.block_on(ToolServerStandIn::with(ToolServerScript { wants, ..script }));
    let stack = Stack::start();
    let client = stack.client();
    let feed = RecordsFeed::start(&client);
    let first = sign_in(&client, &stand_in, new(&stand_in));
    let (_, chat) = stack.workspace(&client);
    let both = format!("mcp:Linear_Northwind/close_issue {{}}; {CALL}");
    let is = |status| {
        let auth = ToolServerAuth::SignIn { status };
        feed.snapshot().tool_server(&"t".into()).is_some_and(|server| server.auth == auth)
    };
    let found = ("Found issue 7".to_string(), false);

    let before = calls::run(&client, &chat.id, &both, 1);
    wait_until("the record says it wants more access", || is(ToolSignInStatus::NeedsMoreAccess));
    // So only what the hub kept says what to ask for, and only that is granted.
    stand_in.change(|script| (script.challenge_scope, script.allows) = (None, None));
    let again = sign_in(&client, &stand_in, ToolSignInTo::Again { id: "t".into() });
    wait_until("the record says it works", || is(ToolSignInStatus::Works));
    // Its worker is sent the new token while it runs.
    let (mut turn, mut after) = (1, Vec::new());
    wait_until("the call works", || {
        turn += 1;
        after = calls::run(&client, &chat.id, &both, turn);
        after.iter().all(|(_, is_error)| !is_error)
    });

    let [(said, true), found_before] = &before[..] else { panic!("{before:?}") };
    let expected = "Linear (Northwind) wants more access for this than its sign-in has (403). Sign in to it again from \
                    Settings › MCP servers.";
    assert!(said.contains(expected), "{said}");
    assert_eq!(*found_before, found);
    assert_eq!(after, [("Closed issue 7".to_string(), false), found]);
    [first, again]
}

/// How the chat's agent last said each of its servers is, where the hub knows.
pub(crate) fn states(client: &Client, chat: &SessionId) -> Vec<(String, ServerState)> {
    let listed = session_records(client, chat).observations.into_iter();
    let servers = listed.filter_map(|observation| match observation {
        Observation::SessionToolServers { servers: Observed::AsOf { value, .. }, .. } => Some(value),
        _ => None,
    });
    servers.flatten().map(|server| (server.name.to_string(), server.state)).collect()
}

/// Each agent was given the server as it launched, the second one that needed signing in to by then, so the CLI of each
/// failed to connect to it. The worker refuses it until it has the token, and each is asked once, so it's connected only
/// if the worker had the token first. The server has been given another name since, which neither agent knows it by.
#[test]
fn a_running_agent_is_connected_to_a_server_once_the_person_signs_in_to_it() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let stand_in = runtime.block_on(ToolServerStandIn::with(ToolServerScript::default()));
    let stack = Stack::start();
    let client = stack.client();
    sign_in(&client, &stand_in, new(&stand_in));
    stand_in.change(|script| script.refreshing = ToolServerRefreshing::Refused("invalid_grant"));
    stand_in.end_access_tokens();
    let (workspace, first) = stack.workspace(&client);
    let is = |state| [("Linear_Northwind".to_string(), state)];
    let failed = |chat: &SessionId| {
        let answered = calls::run(&client, chat, CALL, 1);
        assert!(answered[0].1 && answered[0].0.contains("Sign in"), "{answered:?}");
        wait_until("the server to have failed", || states(&client, chat) == is(ServerState::Failed));
    };
    failed(&first.id);
    let second = client.session_create(&workspace.id, AgentKind::Claude, Settings::default(), None).unwrap();
    failed(&second.id);

    client
        .call(ToolServerEdit {
            id: "t".into(),
            name: Some("Linear".into()),
            url: None,
            auth: None,
            saved_anyway: false,
        })
        .unwrap();
    sign_in(&client, &stand_in, ToolSignInTo::Again { id: "t".into() });

    for chat in [first.id, second.id] {
        wait_until("it to be connected", || states(&client, &chat) == is(ServerState::Connected));
        assert_eq!(calls::run(&client, &chat, CALL, 2), [("Found issue 7".to_string(), false)]);
        assert_eq!(states(&client, &chat), is(ServerState::Connected), "as its next turn says too");
    }
}

/// The next hub has which servers the agent was given, and how each is, as the last hub heard.
#[test]
fn an_agent_the_hub_carried_on_with_is_connected_to_a_server_signed_in_to_since() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let stand_in = runtime.block_on(ToolServerStandIn::with(ToolServerScript::default()));
    let mut stack = Stack::start();
    let client = stack.client();
    sign_in(&client, &stand_in, new(&stand_in));
    stand_in.change(|script| script.refreshing = ToolServerRefreshing::Refused("invalid_grant"));
    stand_in.end_access_tokens();
    let (_, chat) = stack.workspace(&client);
    let is = |state| [("Linear_Northwind".to_string(), state)];
    assert!(calls::run(&client, &chat.id, CALL, 1)[0].1);
    wait_until("the server to have failed", || states(&client, &chat.id) == is(ServerState::Failed));

    update(&mut stack);
    let client = stack.client();
    sign_in(&client, &stand_in, ToolSignInTo::Again { id: "t".into() });

    wait_until("it to be connected", || states(&client, &chat.id) == is(ServerState::Connected));
    let started =
        read_all(&client, &chat.id).iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the same agent");
}
