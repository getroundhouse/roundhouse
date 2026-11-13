//! The conformance scenarios, replayed through the adapter's own process code from what the CLI said when they were
//! played on the scripted model, in `../fixtures/claude/`, so their checks run without the CLI. Then the replay itself,
//! on captures changed so the adapter no longer does what they say. `cargo run -p agents --example conformance --
//! --save` captures them again, as `docs/conformance.md` says.

#![cfg(test)]

use agents::claude::conformance::replay;
use agents::conformance::scenarios;
use serde_json::{Value, json};

const PLAIN_TURN: &str = include_str!("../fixtures/claude/plain_turn.scripted.jsonl");

#[tokio::test]
async fn a_plain_turn() {
    replay(&scenarios::PLAIN_TURN, PLAIN_TURN).await.assert_passed();
}

#[tokio::test]
async fn a_command_cancelled_then_the_next_prompt() {
    let fixture = include_str!("../fixtures/claude/cancel_command.scripted.jsonl");
    replay(&scenarios::CANCEL_COMMAND, fixture).await.assert_passed();
}

#[tokio::test]
async fn a_conversation_resumed_by_the_next_process() {
    replay(&scenarios::RESUME, include_str!("../fixtures/claude/resume.scripted.jsonl")).await.assert_passed();
}

#[tokio::test]
async fn thinking_recorded_with_its_reply() {
    replay(&scenarios::THINKING, include_str!("../fixtures/claude/thinking.scripted.jsonl")).await.assert_passed();
}

#[tokio::test]
async fn a_plan_approved() {
    let fixture = include_str!("../fixtures/claude/plan_approved.scripted.jsonl");
    replay(&scenarios::PLAN_APPROVED, fixture).await.assert_passed();
}

const DID_WHAT_IT_DID: &str = "the adapter did what it did when the trial was captured";

/// The plain turn's capture, with `change` made to its entries.
fn changed(change: impl FnOnce(&mut Vec<Value>)) -> String {
    let mut entries: Vec<Value> = PLAIN_TURN.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    change(&mut entries);
    entries.iter().map(|entry| entry.to_string() + "\n").collect()
}

fn position(entries: &[Value], matches: impl Fn(&Value) -> bool) -> usize {
    entries.iter().position(matches).unwrap()
}

#[tokio::test]
async fn a_replay_fails_when_the_adapter_writes_what_the_capture_doesnt_have() {
    let fixture = changed(|entries| {
        let prompt = position(entries, |entry| entry["to_cli"]["type"] == "user");
        entries.remove(prompt);
    });

    let report = replay(&scenarios::PLAIN_TURN, &fixture).await;

    report.assert_failed(DID_WHAT_IT_DID, "the adapter also wrote user");
}

#[tokio::test]
async fn a_replay_fails_when_the_adapter_doesnt_write_what_the_capture_has() {
    let fixture = changed(|entries| {
        let result = position(entries, |entry| entry["from_cli"]["type"] == "result");
        let interrupt = json!({"type": "control_request", "request_id": "r", "request": {"subtype": "interrupt"}});
        entries.insert(result + 1, json!({"to_cli": interrupt}));
    });

    let report = replay(&scenarios::PLAIN_TURN, &fixture).await;

    report.assert_failed(DID_WHAT_IT_DID, "the adapter never wrote control_request/interrupt");
}

#[tokio::test]
async fn a_replay_fails_when_the_adapter_writes_a_line_otherwise() {
    let fixture = changed(|entries| {
        let prompt = position(entries, |entry| entry["to_cli"]["type"] == "user");
        entries[prompt]["to_cli"]["message"]["content"][0]["text"] = json!("Something else.");
    });

    let report = replay(&scenarios::PLAIN_TURN, &fixture).await;

    report.assert_failed(DID_WHAT_IT_DID, "the adapter wrote user as");
}

#[tokio::test]
async fn a_replay_fails_when_the_adapter_launches_the_cli_otherwise() {
    let fixture = changed(|entries| {
        let args = entries[0]["launch"]["args"].as_array_mut().unwrap();
        let effort = args.iter().position(|arg| arg == "--effort").unwrap();
        args[effort + 1] = json!("max");
    });

    let report = replay(&scenarios::PLAIN_TURN, &fixture).await;

    report.assert_failed(DID_WHAT_IT_DID, "the adapter launched the CLI with");
}
