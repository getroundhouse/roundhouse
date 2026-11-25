//! The agent's tool servers, as its adapter reports them of what Codex says as each starts and answers of each.

use protocol::capability::Report;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::Harness;
use super::script::{item, turn};
use crate::testing::Asking;
use crate::{AgentOutput, AgentProcess};

/// The server `name` of the thread `thread` having moved to `status`.
fn moved(thread: &str, name: &str, status: &str, error: Option<&str>) -> Value {
    json!({"method":"mcpServer/startupStatus/updated",
        "params":{"threadId":thread,"name":name,"status":status,"error":error,"failureReason":null}})
}

/// What Codex answers of the server `name`, asked of it alone in the agent's own thread.
fn answers(name: &str, status: &str, tools: &Value) -> Value {
    json!({"on":"mcpServerStatus/list",
        "params":{"threadId":"thread","serverName":name,"detail":"toolsAndAuthOnly"},
        "actions":[{"result":{"data":[{"name":name,"runtimeStatus":status,"tools":tools,"toolsError":null}]}}]})
}

/// Codex's answer to a prompt: the turn `id`, in which it says `said`.
fn turn_saying(id: &str, said: Vec<Value>) -> Vec<Value> {
    let mut actions = vec![
        json!({"result":{"turn":{"id":id}}}),
        turn("thread", id, "inProgress"),
        item("thread", id, json!({"type":"userMessage"}), true),
    ];
    actions.extend(said);
    actions.push(turn("thread", id, "completed"));
    actions
}

/// The servers the adapter reports next.
async fn reported(h: &mut Harness) -> Vec<ToolServerState> {
    loop {
        if let AgentOutput::Report(Report::ToolServers { servers }) = h.next().await {
            return servers;
        }
    }
}

/// Whole each time, and again as each of the agent's turns starts. Roundhouse's own is left out, and so is what a
/// subagent's thread says of its own.
#[tokio::test]
async fn the_agents_servers_are_reported_as_codex_says_them_and_with_what_it_answers() {
    let child = json!({"type":"subAgentActivity","id":"call","kind":"started","agentThreadId":"child",
        "agentPath":"/root/child","model":"gpt-6-luna","reasoningEffort":"low"});
    let said = vec![
        item("thread", "first", child, true),
        moved("thread", "tickets", "starting", None),
        moved("thread", "roundhouse", "ready", None),
        moved("thread", "docs", "ready", None),
        moved("thread", "gone", "failed", Some("sending to https://mcp.example.com/?key=secret")),
        moved("thread", "tickets", "failed", Some("The tickets MCP server is not logged in.")),
        moved("child", "docs", "failed", Some("refused")),
    ];
    let mut h = Harness::new(&json!([
        {"on":"turn/start","at":1,"actions":turn_saying("first", said)},
        {"on":"turn/start","at":2,"actions":turn_saying("second", Vec::new())},
        answers("docs", "connected", &json!({"find":{},"read":{}})),
        answers("gone", "failed", &json!({})),
        answers("tickets", "authenticationRequired", &json!({})),
    ]));
    let agent = h.started().await;

    agent.prompt(Seq::FIRST, &Prompt::new("Look it up")).await.unwrap();

    assert_eq!(reported(&mut h).await, [], "none, as the turn starts");
    let is = |name: &str, state, error: Option<&str>, tools| ToolServerState {
        name: name.into(),
        state,
        error: error.map(str::to_owned),
        tools,
    };
    assert_eq!(reported(&mut h).await, [is("tickets", ServerState::Starting, None, None)]);
    let whole = [
        is("docs", ServerState::Connected, None, Some(2)),
        is("gone", ServerState::Failed, Some("sending to https://mcp.example.com"), None),
        is("tickets", ServerState::NeedsSignIn, None, None),
    ];
    while reported(&mut h).await != whole {}

    agent.prompt(Seq::new(2), &Prompt::new("And again")).await.unwrap();

    assert_eq!(reported(&mut h).await, whole, "again, as the next turn starts");
    agent.stop().await;
    let sent = std::fs::read_to_string(h.room.path().join("native-requests.jsonl")).unwrap();
    let mut asked: Vec<String> = sent
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|request| request["method"] == "mcpServerStatus/list")
        .map(|request| request["params"]["serverName"].as_str().unwrap().to_owned())
        .collect();
    asked.sort();
    assert_eq!(asked, ["docs", "gone", "tickets"], "each once, in its own thread, and never Roundhouse's own");
}
