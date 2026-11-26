//! A server the person moves to another URL while an agent is connected to it.

use std::time::Duration;

use http_body_util::{BodyExt, Either, Full};
use hyper::body::Bytes;
use hyper::{Method, Response, StatusCode};
use logging::testing::Logs;
use parking_lot::Mutex;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::model::RunId;
use protocol::worker::{AskReply, ProxiedServer, ToolsToken};
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::signed_in::{FIRST, answered_by, with};
use super::stand_in::{Heard, url};
use super::{Agent, Answer, Events, Reply, StandIn};
use crate::tools::Caller;

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}"#;
const DISCOVER: &str = r#"{"jsonrpc":"2.0","id":0,"method":"server/discover"}"#;
const CALL: &str = r#"{"jsonrpc":"2.0","id":"call-7","method":"tools/call","params":{"name":"list"}}"#;
const LISTEN: &str = r#"{"jsonrpc":"2.0","id":"listen-1","method":"subscriptions/listen","params":{}}"#;
const NOTIFICATION: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const MOVED: &str = "Linear (Northwind) has been moved to another URL in Roundhouse, so nothing is sent to it until this agent connects to it again.";

/// One that gives each agent the session `session`, and answers a message with itself.
async fn giving(session: &'static str) -> StandIn {
    StandIn::answering(move |asked| {
        let reply = Response::builder().header("mcp-session-id", session);
        reply.body(Either::Left(Full::new(asked.body.clone()))).unwrap()
    })
    .await
}

/// One that gives no sessions.
async fn giving_none() -> StandIn {
    StandIn::answering(|asked| Response::new(Either::Left(Full::new(asked.body.clone())))).await
}

async fn send(agent: &Agent, method: Method, session: &str, body: &'static str) -> Answer {
    let path = agent.path().await;
    Answer::of(agent.request(method, &path, &[("mcp-session-id", session)], body).await).await
}

/// Connected to `server`, in the session it gave.
async fn connected(server: &StandIn, session: &'static str) -> Agent {
    let agent = Agent::with(vec![server.as_set()]).await;
    let answer = send(&agent, Method::POST, "", INITIALIZE).await;
    assert_eq!((answer.status, answer.header("mcp-session-id")), (StatusCode::OK, session));
    agent
}

fn move_to(agent: &Agent, server: &StandIn) {
    agent.tools.proxy.set(vec![server.as_set()]);
}

/// Nothing of the old server's reaches the new one, by any of the transport's methods, and what's refused is refused as
/// the worker refuses anything. A CLI takes 404 as its session having ended, and sends the call again.
#[tokio::test]
async fn an_agent_connected_to_a_server_thats_moved_is_cut_off_from_it() {
    let (before, after) = (giving("before").await, giving("after").await);
    let agent = connected(&before, "before").await;

    move_to(&agent, &after);

    for session in ["before", ""] {
        let (id, said) = send(&agent, Method::POST, session, CALL).await.error();
        assert_eq!((id, said.as_str()), ("call-7".into(), MOVED));
        for (method, body) in [(Method::POST, NOTIFICATION), (Method::GET, ""), (Method::DELETE, "")] {
            let refused = send(&agent, method, session, body).await;
            assert_eq!((refused.status, refused.body.as_str()), (StatusCode::BAD_GATEWAY, MOVED));
        }
    }
    let (_, said) = send(&agent, Method::POST, "before", INITIALIZE).await.error();
    assert_eq!(said, MOVED, "it goes on with the session it had");
    assert!(after.heard().is_empty(), "{:?}", after.heard());
    assert_eq!(before.heard().len(), 1, "nor does it reach the old one");
}

#[tokio::test]
async fn an_agent_that_connects_afresh_has_the_server_where_its_moved() {
    for handshake in [INITIALIZE, DISCOVER] {
        let (before, after) = (giving("before").await, giving("after").await);
        let agent = connected(&before, "before").await;
        move_to(&agent, &after);

        let connecting = send(&agent, Method::POST, "", handshake).await;
        let called = send(&agent, Method::POST, "after", CALL).await;

        assert_eq!((connecting.body.as_str(), connecting.header("mcp-session-id")), (handshake, "after"));
        assert_eq!((called.status, called.body.as_str()), (StatusCode::OK, CALL));
        assert_eq!(before.heard().len(), 1);
    }
}

/// Another of its connections may go on with it, as another of Codex's threads does.
#[tokio::test]
async fn a_session_of_the_server_before_stays_cut_off_once_the_agent_has_connected_afresh() {
    let (before, after) = (giving("before").await, giving("after").await);
    let agent = connected(&before, "before").await;
    move_to(&agent, &after);

    send(&agent, Method::POST, "", INITIALIZE).await;

    let (_, said) = send(&agent, Method::POST, "before", CALL).await.error();
    let streamed = send(&agent, Method::GET, "before", "").await;
    let ended = send(&agent, Method::DELETE, "before", "").await;
    assert_eq!(said, MOVED);
    assert_eq!((streamed.status, ended.status), (StatusCode::BAD_GATEWAY, StatusCode::BAD_GATEWAY));
    let heard: Vec<_> = after.heard().into_iter().map(|heard| heard.body).collect();
    assert_eq!(heard, [INITIALIZE], "only what connected");
}

/// Claude asks whether a server speaks the newer protocol first.
#[tokio::test]
async fn a_handshake_thats_refused_lets_nothing_else_in() {
    let before = giving_none().await;
    let after = StandIn::answering(|_| refused()).await;
    let agent = Agent::with(vec![before.as_set()]).await;
    send(&agent, Method::POST, "", DISCOVER).await;
    move_to(&agent, &after);

    let probed = send(&agent, Method::POST, "", DISCOVER).await;

    assert_eq!(probed.status, StatusCode::BAD_REQUEST);
    let (_, said) = send(&agent, Method::POST, "", CALL).await.error();
    assert_eq!(said, MOVED);
    let heard: Vec<_> = after.heard().into_iter().map(|heard| heard.body).collect();
    assert_eq!(heard, [DISCOVER]);
}

fn refused() -> Reply {
    Response::builder().status(400).body(Either::Left(Full::default())).unwrap()
}

/// Nor when anything else about it does, such as its name or its key.
#[tokio::test]
async fn a_query_that_changes_cuts_no_agent_off() {
    let server = giving("before").await;
    let agent = connected(&server, "before").await;
    let queried = url(&format!("http://127.0.0.1:{}/mcp?tools=made-up", server.port));

    agent.tools.proxy.set(vec![ProxiedServer { url: queried, header: None, ..server.as_set() }]);

    let called = send(&agent, Method::POST, "before", CALL).await;
    assert_eq!((called.status, called.body.as_str()), (StatusCode::OK, CALL));
    assert_eq!(server.heard().last().map(|heard: &Heard| heard.path.clone()).unwrap(), "/mcp?tools=made-up");
}

/// Moved to another and back, it's the server the agent connected to.
#[tokio::test]
async fn an_agent_goes_on_with_a_server_thats_moved_back() {
    let (before, after) = (giving("before").await, giving("after").await);
    let agent = connected(&before, "before").await;

    move_to(&agent, &after);
    move_to(&agent, &before);

    assert_eq!(send(&agent, Method::POST, "before", CALL).await.body, CALL);
}

#[tokio::test]
async fn an_agent_that_starts_has_each_server_where_it_is() {
    let (before, after) = (giving("before").await, giving("after").await);
    let agent = connected(&before, "before").await;
    move_to(&agent, &after);

    let caller = Caller { session_id: "s".into(), worktree: "/no/worktree".into() };
    agent.tools.admit(ToolsToken::new("token-s"), caller, RunId::new(2));

    assert_eq!(send(&agent, Method::POST, "", CALL).await.body, CALL);
}

/// As its run ends, so the worker keeps nothing of an agent that has gone, but not as an earlier run's ends after the
/// next has started.
#[tokio::test]
async fn what_the_worker_knows_of_an_agent_goes_as_its_run_ends() {
    let (before, after) = (giving("before").await, giving("after").await);
    let agent = connected(&before, "before").await;
    let session_id = "s".into();
    let place = |server: &StandIn| Some(server.as_set().url.place());
    let connected_to =
        |agent: &Agent| agent.tools.proxy.saying_where(&session_id, vec![hubs()])[0].connected_to.clone();
    move_to(&agent, &after);

    agent.tools.proxy.ended(&session_id, RunId::new(2));
    let not_its_run = connected_to(&agent);
    agent.tools.proxy.ended(&session_id, RunId::new(1));
    let its_run = connected_to(&agent);
    let caller = Caller { session_id: "s".into(), worktree: "/no/worktree".into() };
    agent.tools.admit(ToolsToken::new("token-s"), caller, RunId::new(2));
    send(&agent, Method::POST, "", INITIALIZE).await;
    agent.tools.proxy.ended(&session_id, RunId::new(1));

    assert_eq!(not_its_run, place(&before));
    assert_eq!(its_run, None);
    assert_eq!(connected_to(&agent), place(&after), "the next run's is kept");
}

/// Its messages come from the server before, and it's open for as long as the agent runs, whichever way its protocol
/// has it asked for. A call under way goes on. The same once the server is removed, or left out of the set.
#[tokio::test]
async fn a_servers_stream_ends_as_its_moved_or_removed_and_a_calls_answer_goes_on() {
    let after = giving("after").await;
    for removed in [false, true] {
        let (senders, streams): (Vec<_>, Vec<_>) = (0..3).map(|_| mpsc::channel(1)).unzip();
        let [get, listening, call] = <[_; 3]>::try_from(senders).unwrap();
        let streams = Mutex::new(streams.into_iter());
        let before = StandIn::answering(move |_| {
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Either::Right(Events(streams.lock().next().unwrap())))
                .unwrap()
        })
        .await;
        let agent = Agent::with(vec![before.as_set()]).await;
        let path = agent.path().await;
        let got = agent.request(Method::GET, &path, &[], "").await;
        let listened = agent.request(Method::POST, &path, &[], LISTEN).await;
        let mut answer = agent.request(Method::POST, &path, &[], CALL).await;
        let within = Duration::from_secs(10);

        if removed {
            agent.tools.proxy.set(vec![]);
        } else {
            move_to(&agent, &after);
        }

        for (mut stream, servers) in [(got, get), (listened, listening)] {
            let ended = timeout(within, stream.frame()).await.expect("the stream to end");
            assert!(ended.is_none(), "{ended:?}");
            timeout(within, servers.closed()).await.expect("the server's end of it to end");
        }
        call.send(Bytes::from_static(b"data: 1\n\n")).await.unwrap();
        let event = timeout(within, answer.frame()).await.expect("the answer").unwrap().unwrap();
        assert_eq!(event.into_data().unwrap(), "data: 1\n\n");
    }
}

/// A call that's refused as it's moved isn't made again where it's moved to.
#[tokio::test]
async fn a_call_isnt_made_again_where_its_server_has_moved() {
    let refusing = StandIn::answering(|_| {
        Response::builder().status(StatusCode::UNAUTHORIZED).body(Either::Left(Full::default())).unwrap()
    })
    .await;
    let after = giving("after").await;
    let renewed = with(&after, "made-up-second", FIRST.next());
    let set = with(&refusing, "made-up-first", FIRST);
    let (agent, _asks) = answered_by(set, move |_| Ok(AskReply::ToolServer(renewed.clone()))).await;

    let (_, said) = agent.post(CALL).await.error();

    assert_eq!(said, MOVED);
    assert!(after.heard().is_empty(), "{:?}", after.heard());
}

/// What an agent says of the only server, which is the hub's.
fn hubs() -> ToolServerState {
    ToolServerState {
        name: "linear_northwind".into(),
        hubs: Some("t-1".into()),
        connected_to: None,
        state: ServerState::Connected,
        error: None,
        tools: None,
        connects_again: None,
    }
}

/// So the app can tell that a running agent is cut off.
#[tokio::test]
async fn what_an_agent_says_of_its_servers_is_passed_on_with_where_its_connected() {
    let (before, after) = (giving("before").await, giving("after").await);
    let says = hubs;
    let own = || ToolServerState { name: "own".into(), hubs: None, ..says() };
    let agent = connected(&before, "before").await;
    let place = |server: &StandIn| Some(server.as_set().url.place());
    let said = |agent: &Agent| {
        let said = agent.tools.proxy.saying_where(&"s".into(), vec![says(), own()]);
        said.into_iter().map(|server| server.connected_to).collect::<Vec<_>>()
    };

    assert_eq!(said(&agent), [place(&before), None]);
    move_to(&agent, &after);
    assert_eq!(said(&agent), [place(&before), None], "it's cut off");
    send(&agent, Method::POST, "", INITIALIZE).await;
    assert_eq!(said(&agent), [place(&after), None]);
    assert_eq!(agent.tools.proxy.saying_where(&"other".into(), vec![says()])[0].connected_to, None);
}

/// Claude asks after a task of the old server's each second.
#[tokio::test]
async fn an_agent_thats_cut_off_is_logged_once() {
    let logs = Logs::capture();
    let (before, after) = (giving("before").await, giving("after").await);
    let agent = connected(&before, "before").await;
    move_to(&agent, &after);

    for _ in 0..3 {
        send(&agent, Method::POST, "before", CALL).await.error();
    }

    let cut_off = logs.text().lines().filter(|line| line.contains("tool_server.cut_off")).count();
    assert_eq!(cut_off, 1);
}
