//! Codex's messages, as a test scripts them for the fake app server to send, and playing them to the adapter, for
//! every test of what the adapter makes of them.

use crate::testing::Asking;
use protocol::event::EventBody;
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::harness::Harness;
use crate::{AgentOutput, AgentProcess};

/// The turn `id` of the thread `thread` starting, while `status` is `inProgress`, or ending with it.
pub(super) fn turn(thread: &str, id: &str, status: &str) -> Value {
    json!({"method":if status == "inProgress" {"turn/started"} else {"turn/completed"},
        "params":{"threadId":thread,"turn":{"id":id,"status":status}}})
}

/// The item `item` of the turn `turn` starting, or ending when it's `done`.
pub(super) fn item(thread: &str, turn: &str, item: Value, done: bool) -> Value {
    let mut value = json!({"method":if done {"item/completed"} else {"item/started"},
        "params":{"threadId":thread,"turnId":turn}});
    value["params"]["item"] = item;
    value
}

/// The agent's message `text`, whole.
pub(super) fn message(thread: &str, turn: &str, text: &str) -> Value {
    item(thread, turn, json!({"type":"agentMessage","id":format!("message-{thread}-{turn}"),"text":text}), true)
}

/// The command `call` running `echo hello`, or, once it's `done`, having printed `hello` and exited 0.
pub(super) fn command(thread: &str, turn: &str, call: &str, done: bool) -> Value {
    item(
        thread,
        turn,
        json!({"type":"commandExecution","id":call,"command":"echo hello","cwd":".",
        "commandActions":[],"status":if done {"completed"} else {"inProgress"},
        "aggregatedOutput":if done {Some("hello")} else {None},"exitCode":if done {Some(0)} else {None}}),
        done,
    )
}

/// Plays `actions` as Codex's answer to the prompt, which starts with the root turn. Codex takes the prompt as the turn
/// starts, which is put in after it when `actions` don't say it, so no test sees the prompt dropped, at whatever point
/// its answer is read.
pub(super) async fn play(mut actions: Vec<Value>) -> Vec<AgentOutput> {
    assert_eq!(actions.first(), Some(&turn("thread", "root-turn", "inProgress")), "the root turn starts first");
    let taken = item("thread", "root-turn", json!({"type":"userMessage"}), true);
    if !actions.contains(&taken) {
        actions.insert(1, taken);
    }
    actions.insert(0, json!({"result":{"turn":{"id":"root-turn"}}}));
    actions.push(json!({"method":"test/finished","params":{}}));
    let mut h = Harness::new(&json!([{"on":"turn/start","actions":actions}]));
    let agent = h.started().await;
    agent.prompt(Seq::FIRST, &Prompt::new("Delegate some work")).await.unwrap();
    let mut output = Vec::new();
    loop {
        let next = h.next().await;
        if matches!(&next, AgentOutput::Event {body:EventBody::AgentOther {kind,..},..} if kind == "test/finished") {
            break;
        }
        output.push(next);
    }
    agent.stop().await;
    output
}

/// The events among `output`.
pub(super) fn bodies(output: &[AgentOutput]) -> Vec<&EventBody> {
    output.iter().filter_map(|out| if let AgentOutput::Event { body, .. } = out { Some(body) } else { None }).collect()
}
