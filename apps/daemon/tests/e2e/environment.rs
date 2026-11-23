//! What the stack's processes run in: the environment a test gives them, and nothing of its own.

#![cfg(test)]

use std::path::{Path, PathBuf};

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

/// What a hub merges over its own environment: what the user's login shell has once it has read their profile, started
/// in the hub's environment and in the user's home. Here that's `/bin/sh`, whose `.profile` adds to `PATH`, exports a
/// variable built on one the test set for the hub, says where it started, and exports one of a parent Claude session's.
const PROFILE: &str = "export PATH=\"$HOME/profile-bin:$PATH\"\nexport FROM_PROFILE=\"$SET_FOR_THE_HUB-then-profile\"\n\
                       export STARTED_IN=\"$(pwd)\"\nexport CLAUDE_CODE_ENTRYPOINT=profiles\n";

/// What a parent Claude session sets, the first two in the hub's environment, as when it's started from inside one,
/// and the last by the profile.
const PARENT_SESSIONS: [&str; 3] = ["CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_ENTRYPOINT"];

/// An agent and a setup script have what the profile exports, built on what the hub was started with, and the
/// profile's `PATH` entry before the hub's own. A hub started from inside a Claude session, such as by an agent, has
/// what the session sets for what it starts, and the daemon forgets it as it starts, whatever it runs as, so neither has
/// it, even when the profile exports it. The user's own login settings stay.
#[test]
fn what_the_stack_starts_has_the_profiles_environment_and_no_parent_claude_sessions() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".profile"), PROFILE).unwrap();
    let hubs = vec![
        ("HOME", home.path().to_str().unwrap().to_string()),
        ("SHELL", "/bin/sh".to_string()),
        ("SET_FOR_THE_HUB", "hubs".to_string()),
        ("CLAUDECODE", "1".to_string()),
        ("CLAUDE_CODE_SESSION_ID", "parent".to_string()),
        ("ANTHROPIC_API_KEY", "kept".to_string()),
    ];
    let stack = Stack::start_with(hubs);
    commit_settings(&stack, "[scripts]\nsetup = \"env > setup-env.txt\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);

    assert_eq!(wait_for_setup(&client, &workspace.id), SetupStatus::Succeeded);
    let names = PARENT_SESSIONS.iter().chain(&["SET_FOR_THE_HUB", "FROM_PROFILE", "ANTHROPIC_API_KEY"]);
    let asked: Vec<String> = names.map(|name| format!("env:{name}; ")).collect();
    // `PATH` last, so it's read to the end of what the agent says.
    let asked = format!("{}env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS; env:PATH", asked.concat());
    client.session_prompt(&session.id, &asked, None).unwrap();
    let agent = assistant_text(&wait_for_turn_end(&client, &session.id, 1));

    let script = std::fs::read_to_string(PathBuf::from(workspace.path.unwrap()).join("setup-env.txt")).unwrap();
    let script_has = |name: &str| script.lines().find_map(|line| line.strip_prefix(&format!("{name}=")));
    for name in PARENT_SESSIONS {
        assert_eq!(script_has(name), None, "the setup script has {name}: {script}");
        assert!(agent.contains(&format!("{name}=<unset>")), "the agent has {name}: {agent}");
    }
    // As a path, since `pwd` can say it through a symbolic link, as macOS's temporary folders are.
    let started_in = script_has("STARTED_IN").map(|dir| std::fs::canonicalize(dir).unwrap());
    assert_eq!(started_in, Some(std::fs::canonicalize(home.path()).unwrap()), "the login shell's folder");
    let values = [("SET_FOR_THE_HUB", "hubs"), ("FROM_PROFILE", "hubs-then-profile"), ("ANTHROPIC_API_KEY", "kept")];
    for (name, value) in values {
        assert_eq!(script_has(name), Some(value), "the setup script's: {script}");
        assert!(agent.contains(&format!("{name}={value}")), "the agent's: {agent}");
    }
    assert!(agent.contains("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1"), "{agent}");
    let profiles = home.path().join("profile-bin");
    for (whose, path) in
        [("the setup script", script_has("PATH")), ("the agent", agent.rsplit_once("PATH=").map(|(_, path)| path))]
    {
        assert!(has_before(path.unwrap(), &profiles, process::testing::logged_out_gh()), "{whose}'s PATH");
    }
}

/// Whether `path` has `first` before `then`.
fn has_before(path: &str, first: &Path, then: &Path) -> bool {
    let entries: Vec<PathBuf> = std::env::split_paths(path).collect();
    let at = |entry: &Path| entries.iter().position(|its| its == entry);
    matches!((at(first), at(then)), (Some(first), Some(then)) if first < then)
}
