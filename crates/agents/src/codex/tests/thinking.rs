use protocol::event::{ContentBlock, EventBody};
use protocol::model::Seq;
use protocol::prompt::Prompt;
use protocol::tail::DeltaChange;
use serde_json::json;

use super::harness::Harness;
use crate::{AgentOutput, AgentProcess};

#[tokio::test]
async fn thinking_is_requested_from_codex_and_preserved_as_summary_text() {
    let mut h = Harness::new(&json!([{"on":"turn/start","actions":[
        {"result":{"turn":{"id":"turn"}}},
        {"method":"turn/started","params":{"threadId":"thread","turn":{"id":"turn"}}},
        {"method":"item/started","params":{"threadId":"thread","turnId":"turn","item":{"type":"reasoning","id":"thought","summary":[]}}},
        {"method":"item/reasoning/summaryTextDelta","params":{"threadId":"thread","turnId":"turn","itemId":"thought","delta":"Checking the paths."}},
        {"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":{"type":"reasoning","id":"thought","summary":["Checking the paths."]}}},
        {"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"completed"}}}
    ]}]));
    let agent = h.started().await;
    let launch = h.request("thread/start").await;
    assert_eq!(launch["params"]["config"]["model_reasoning_summary"], "auto");
    agent.send_prompt(Seq::FIRST, &Prompt::new("Think it through")).await.unwrap();
    let start = h.request("turn/start").await;
    assert_eq!(start["params"]["summary"], "auto");
    let mut streamed = false;
    let mut recorded = false;
    loop {
        match h.next().await {
            AgentOutput::Delta(delta) => {
                streamed |= matches!(delta.change, DeltaChange::Thinking {text,..} if text=="Checking the paths.");
            }
            AgentOutput::Event { body: EventBody::Assistant { blocks, .. }, .. } => {
                recorded |=
                    blocks.iter().any(|b| matches!(b,ContentBlock::Thinking{text} if text=="Checking the paths."));
            }
            AgentOutput::Event { body: EventBody::TurnEnded { .. }, .. } => break,
            AgentOutput::Event { .. }
            | AgentOutput::Context(_)
            | AgentOutput::Cache(_)
            | AgentOutput::Limits { .. }
            | AgentOutput::Exited { .. } => {}
        }
    }
    assert!(streamed && recorded);
    agent.stop().await;
}
