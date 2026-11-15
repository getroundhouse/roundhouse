//! Native ACP JSON-RPC captures through the supervised Antigravity adapter, without a CLI or account.

#![cfg(test)]

use agents::antigravity::conformance::replay;
use agents::conformance::scenarios;

macro_rules! scenario {
    ($name:ident, $scenario:ident) => {
        #[tokio::test]
        async fn $name() {
            replay(
                &scenarios::$scenario,
                include_str!(concat!("../fixtures/antigravity/", stringify!($name), ".scripted.jsonl")),
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
