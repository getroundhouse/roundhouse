//! What the stack's processes run in: the environment a test gives them, and nothing of its own, and the profile's
//! rather than whoever started the hub's.

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

/// What a hub started from a terminal inside a Claude session, and in tmux, has that the terminal set, none of which
/// a hub passes on.
const STARTERS: [&str; 4] = ["CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "TMUX", "STARTERS_OWN"];

/// What the agent is asked to say of its environment: [`STARTERS`], the profile's variable, and one its adapter sets.
const ASKED: &str = "env:CLAUDECODE; env:CLAUDE_CODE_SESSION_ID; env:TMUX; env:STARTERS_OWN; env:FROM_PROFILE; \
                     env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS";

/// A hub started from a terminal inside a Claude session, and in tmux, with a variable of the terminal's own, passes
/// none of them on, so neither an agent nor a setup script has them. What the user's profile exports, as their login
/// shell has it, reaches both, over the terminal's value, and once the hub is started again without it. The profile
/// makes `env` fail, so reading it doesn't rest on the system's `env`, whose `-0` not every system has.
#[test]
fn what_the_stack_starts_has_the_profiles_environment_and_none_of_whoever_started_it() {
    let home = tempfile::tempdir().unwrap();
    write_profile(home.path(), "env() { return 1; }\nexport FROM_PROFILE=the-profiles");
    let terminals = [
        ("HOME", home.path().to_str().unwrap().to_string()),
        ("SHELL", "/bin/sh".to_string()),
        ("CLAUDECODE", "1".to_string()),
        ("CLAUDE_CODE_SESSION_ID", "parent".to_string()),
        ("TMUX", "/tmp/tmux-501/default,1234,0".to_string()),
        ("STARTERS_OWN", "the-terminals".to_string()),
        ("FROM_PROFILE", "the-terminals".to_string()),
    ];
    let mut stack = Stack::start_with(terminals.to_vec());
    commit_settings(&stack, "[scripts]\nsetup = \"env > setup-env.txt\"\n");

    let first = environments(&stack);
    stack.stop_hub();
    stack.env.remove("FROM_PROFILE");
    stack.start_hub();
    let again = environments(&stack);

    for (agent, script) in [first, again] {
        let script_has = |name: &str| script.lines().find_map(|line| line.strip_prefix(&format!("{name}=")));
        for name in STARTERS {
            assert_eq!(script_has(name), None, "the setup script has {name}");
            assert!(agent.contains(&format!("{name}=<unset>")), "the agent has {name}: {agent}");
        }
        assert_eq!(script_has("FROM_PROFILE"), Some("the-profiles"), "the setup script's: {script}");
        assert!(agent.contains("FROM_PROFILE=the-profiles"), "{agent}");
        assert!(agent.contains("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1"), "{agent}");
    }
}

/// A `.profile` in `home`, which `/bin/sh` reads as a login shell, that sets `PATH` to the test's, after the `gh` that's
/// logged in to nothing, as every stack's is, and then runs `then`.
fn write_profile(home: &Path, then: &str) {
    let tests = std::env::var_os("PATH").unwrap();
    let dirs = std::iter::once(process::testing::logged_out_gh().to_path_buf()).chain(std::env::split_paths(&tests));
    let path = std::env::join_paths(dirs).unwrap().into_string().unwrap();
    std::fs::write(home.join(".profile"), format!("export PATH='{path}'\n{then}\n")).unwrap();
}

/// What the agent of a new workspace says of what it's [`ASKED`], and what the workspace's setup script ran with.
fn environments(stack: &Stack) -> (String, String) {
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    assert_eq!(wait_for_setup(&client, &workspace.id), SetupStatus::Succeeded);
    client.session_prompt(&session.id, ASKED, None).unwrap();
    let agent = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    let script = std::fs::read_to_string(PathBuf::from(workspace.path.unwrap()).join("setup-env.txt")).unwrap();
    (agent, script)
}
