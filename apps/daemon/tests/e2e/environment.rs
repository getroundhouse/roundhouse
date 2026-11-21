//! What the stack's processes run in: the environment a test gives them, and nothing of its own.

#![cfg(test)]

use std::path::PathBuf;

use protocol::model::SetupStatus;

use crate::support::{Stack, assistant_text, commit_settings, wait_for_setup, wait_for_turn_end};

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

/// A hub started from inside a Claude session, such as by an agent, has what the session sets for what it starts, and
/// the daemon forgets it as it starts, so neither an agent nor a setup script has it. The user's own login settings
/// stay.
#[test]
fn nothing_the_stack_starts_has_a_parent_claude_sessions_variables() {
    let parent = vec![
        ("CLAUDECODE", "1".to_string()),
        ("CLAUDE_CODE_SESSION_ID", "parent".to_string()),
        ("ANTHROPIC_API_KEY", "kept".to_string()),
    ];
    let stack = Stack::start_with(parent);
    commit_settings(&stack, "[scripts]\nsetup = \"env > setup-env.txt\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);

    assert_eq!(wait_for_setup(&client, &workspace.id), SetupStatus::Succeeded);
    let asked =
        "env:CLAUDECODE; env:CLAUDE_CODE_SESSION_ID; env:ANTHROPIC_API_KEY; env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS";
    client.session_prompt(&session.id, asked, None).unwrap();
    let agent = assistant_text(&wait_for_turn_end(&client, &session.id, 1));

    let script = std::fs::read_to_string(PathBuf::from(workspace.path.unwrap()).join("setup-env.txt")).unwrap();
    let script_has = |name: &str| script.lines().any(|line| line.starts_with(&format!("{name}=")));
    assert!(!script_has("CLAUDECODE") && !script_has("CLAUDE_CODE_SESSION_ID"), "the setup script's: {script}");
    assert!(script_has("ANTHROPIC_API_KEY"), "the user's own login settings stay: {script}");
    assert!(agent.contains("CLAUDECODE=<unset>") && agent.contains("CLAUDE_CODE_SESSION_ID=<unset>"), "{agent}");
    assert!(agent.contains("ANTHROPIC_API_KEY=kept"), "the user's own login settings stay: {agent}");
    assert!(agent.contains("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1"), "{agent}");
}
