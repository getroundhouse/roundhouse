#![cfg(test)]
use super::harness::Harness;
use crate::{AgentOutput, AgentProcess};
use protocol::event::EventBody;
use protocol::model::Seq;
use protocol::prompt::Prompt;

use serde_json::json;

fn turn(id: &str, complete: bool) -> Vec<serde_json::Value> {
    let mut events = vec![
        json!({"result":{"turn":{"id":id}}}),
        json!({"method":"turn/started","params":{"turn":{"id":id}}}),
        json!({"method":"item/completed","params":{"item":{"type":"userMessage"}}}),
    ];
    if complete {
        events.push(json!({"method":"turn/completed","params":{"turn":{"id":id,"status":"completed"}}}));
    }
    events
}

#[tokio::test]
async fn steering_crosses_completion_in_either_order_without_losing_or_duplicating_the_prompt() {
    let refusal = json!({"error":{"code":-32600,"message":"no active turn"}});
    let completion = json!({"method":"turn/completed","params":{"turn":{"id":"first","status":"completed"}}});
    for order in [vec![refusal.clone(), completion.clone()], vec![completion, refusal]] {
        let mut h = Harness::new(&json!([
            {"on":"turn/start","at":1,"actions":turn("first",false)},
            {"on":"turn/start","at":2,"actions":turn("second",true)},
            {"on":"turn/steer","actions":order}
        ]));
        let agent = h.started().await;
        agent.send_prompt(Seq::FIRST, &Prompt::new("first")).await.unwrap();
        loop {
            if matches!(h.next().await, AgentOutput::Event { body: EventBody::PromptTaken { .. }, .. }) {
                break;
            }
        }
        agent.send_prompt(Seq::new(2), &Prompt::new("second")).await.unwrap();
        let mut done = agent.shared.turn_done.subscribe();
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            done.wait_for(|id| id.as_ref().is_some_and(|id| id.0 == "second")),
        )
        .await
        .unwrap()
        .unwrap();
        agent.stop().await;
        let mut taken = Vec::new();
        let mut starts = 0;
        while let Some(output) = h.rx.recv().await {
            if let AgentOutput::Event { body, raw, .. } = output {
                if let EventBody::PromptTaken { prompt } = body {
                    taken.push(prompt);
                }
                assert!(!matches!(body, EventBody::PromptDropped { .. }), "{body:?}");
                if let EventBody::AgentOther { kind, .. } = body
                    && kind == "test/request"
                {
                    let value: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
                    if value["params"]["method"] == "turn/start" {
                        starts += 1;
                    }
                }
            }
        }
        assert_eq!(taken, [Seq::new(2)]);
        assert_eq!(starts, 1);
    }
}

#[tokio::test]
async fn a_missing_intake_reply_is_settled_after_shutdown_without_a_restartable_refusal() {
    for echo in [false, true] {
        let mut messages = if echo { turn("first", false).into_iter().skip(1).collect::<Vec<_>>() } else { Vec::new() };
        messages.push(json!({"method":"test/ready","params":{}}));
        let mut h = Harness::new(&json!([{"on":"turn/start","actions":messages}]));
        let agent = h.started().await;
        let sender = agent.clone();
        let sending = tokio::spawn(async move { sender.send_prompt(Seq::FIRST, &Prompt::new("first")).await });
        let mut taken = 0;
        loop {
            match h.next().await {
                AgentOutput::Event { body: EventBody::PromptTaken { .. }, .. } => taken += 1,
                AgentOutput::Event { body: EventBody::AgentOther { kind, .. }, .. } if kind == "test/ready" => break,
                AgentOutput::Event { .. }
                | AgentOutput::Delta(_)
                | AgentOutput::Context(_)
                | AgentOutput::Cache(_)
                | AgentOutput::Limits { .. }
                | AgentOutput::Exited { .. } => {}
            }
        }
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(31)).await;
        tokio::time::resume();
        assert!(sending.await.unwrap().is_ok());
        assert!(agent.is_gone());
        let mut dropped = 0;
        while let Some(output) = h.rx.recv().await {
            match output {
                AgentOutput::Event { body: EventBody::PromptTaken { .. }, .. } => taken += 1,
                AgentOutput::Event { body: EventBody::PromptDropped { .. }, .. } => dropped += 1,
                AgentOutput::Event { .. }
                | AgentOutput::Delta(_)
                | AgentOutput::Context(_)
                | AgentOutput::Cache(_)
                | AgentOutput::Limits { .. }
                | AgentOutput::Exited { .. } => {}
            }
        }
        assert_eq!((taken, dropped), if echo { (1, 0) } else { (0, 1) });
    }
}

#[tokio::test]
async fn a_broken_input_pipe_stops_the_process_and_settles_the_prompt() {
    let mut h = Harness::new(&json!([{"on":"thread/start","actions":[
        {"result":{"thread":{"id":"thread"}}},"close-input"
    ]}]));
    let agent = h.started().await;
    loop {
        if let AgentOutput::Event { body: EventBody::AgentOther { kind, .. }, .. } = h.next().await
            && kind == "test/ready"
        {
            break;
        }
    }
    assert!(agent.send_prompt(Seq::FIRST, &Prompt::new("undelivered")).await.is_ok());
    assert!(agent.is_gone());
    let mut dropped = 0;
    while let Some(output) = h.rx.recv().await {
        match output {
            AgentOutput::Event { body: EventBody::PromptTaken { .. }, .. } => panic!("broken pipe took a prompt"),
            AgentOutput::Event { body: EventBody::PromptDropped { .. }, .. } => dropped += 1,
            AgentOutput::Event { .. }
            | AgentOutput::Delta(_)
            | AgentOutput::Context(_)
            | AgentOutput::Cache(_)
            | AgentOutput::Limits { .. }
            | AgentOutput::Exited { .. } => {}
        }
    }
    assert_eq!(dropped, 1);
}
