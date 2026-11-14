//! Native JSON-RPC captures through the supervised Codex adapter, without a CLI or account.
#![cfg(unix)]
use agents::codex::conformance::replay;
use agents::conformance::scenarios;

macro_rules! scenario {
    ($name:ident, $scenario:ident) => {
        #[tokio::test]
        async fn $name() {
            replay(
                &scenarios::$scenario,
                include_str!(concat!("../fixtures/codex/", stringify!($name), ".scripted.jsonl")),
            )
            .await
            .assert_passed();
        }
    };
}
scenario!(plain_turn, PLAIN_TURN);
scenario!(cancel_command, CANCEL_COMMAND);
scenario!(resume, RESUME);
scenario!(thinking, THINKING);
scenario!(plan_approved, PLAN_APPROVED);

#[tokio::test]
async fn a_plan_returned_as_a_final_message_still_needs_approval() {
    let fixture = include_str!("../fixtures/codex/plan_approved.scripted.jsonl")
        .replace("\"type\":\"plan\"", "\"type\":\"agentMessage\"");
    assert_ne!(fixture, include_str!("../fixtures/codex/plan_approved.scripted.jsonl"));
    replay(&scenarios::PLAN_APPROVED, &fixture).await.assert_passed();
}
