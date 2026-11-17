//! What the stack's processes run in: the environment a test gives them, and nothing of its own.

#![cfg(test)]

use std::path::PathBuf;

use crate::support::{Stack, wait_for_turn_end};

/// Whoever runs the tests can have anything in their environment, as Cargo has `CARGO_MANIFEST_DIR` in this test's,
/// and none of it reaches the hub, its workers or what they start.
#[test]
fn nothing_of_the_tests_own_environment_reaches_what_the_stack_starts() {
    assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some(), "Cargo sets it for a test");
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);

    client.session_prompt(&session.id, "tool:env > agent-env.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let env = std::fs::read_to_string(PathBuf::from(workspace.path.unwrap()).join("agent-env.txt")).unwrap();
    assert!(env.lines().any(|line| line.starts_with("PATH=")), "it isn't an environment: {env}");
    assert!(!env.lines().any(|line| line.starts_with("CARGO_MANIFEST_DIR=")), "{env}");
}
