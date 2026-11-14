use protocol::event::{AgentRunState, EventBody};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use serde_json::json;

use super::super::harness::Harness;
use super::fixtures::{activity, command, turn};
use crate::{AgentOutput, AgentProcess};

#[tokio::test]
async fn cancelling_the_root_does_not_settle_child_work_or_target_its_terminals() {
    let mut root_command = command("thread", "root-turn", "root-command", false);
    root_command["params"]["item"]["processId"] = json!("root-process");
    let mut child_command = command("child", "child-turn", "child-command", false);
    child_command["params"]["item"]["processId"] = json!("child-process");
    let mut h = Harness::new(&json!([
        {"on":"turn/start","actions":[
            {"result":{"turn":{"id":"root-turn"}}},turn("thread","root-turn","inProgress"),root_command,
            activity("thread","root-turn","child","spawn","started",true),turn("child","child-turn","inProgress"),child_command,
            {"method":"test/ready","params":{}}
        ]},
        {"on":"turn/interrupt","actions":[{"result":{}},turn("thread","root-turn","interrupted")]},
        {"on":"thread/backgroundTerminals/terminate","actions":[{"result":{}}]},
        {"on":"thread/backgroundTerminals/list","actions":[{"result":{"data":[],"nextCursor":null}}]}
    ]));
    let agent = h.started().await;
    agent.send_prompt(Seq::FIRST, &Prompt::new("Delegate")).await.unwrap();
    loop {
        if matches!(h.next().await,AgentOutput::Event{body:EventBody::AgentOther{kind,..},..} if kind=="test/ready") {
            break;
        }
    }
    agent.interrupt().await.unwrap();
    let stopped = h.request("thread/backgroundTerminals/terminate").await;
    assert_eq!(stopped["params"], json!({"threadId":"thread","processId":"root-process"}));
    loop {
        let out = h.next().await;
        assert!(!matches!(out, AgentOutput::Event { body: EventBody::SubagentEnded { .. }, .. }));
        assert!(
            !matches!(&out,AgentOutput::Event{body:EventBody::ToolCallDropped{tool_use_id},..} if tool_use_id.as_str()=="child-command")
        );
        if matches!(out, AgentOutput::Event { body: EventBody::AgentState { state: AgentRunState::Idle }, .. }) {
            break;
        }
    }
    assert!(agent.shared.state.lock().work(Some(&"spawn".into())).calls.contains_key(&"child-command".into()));
    agent.stop().await;
}
