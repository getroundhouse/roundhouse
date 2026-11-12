//! What every agent is told about working in Roundhouse, and the tmp folder its instructions send it to.

#![cfg(test)]

use std::path::PathBuf;

use crate::support::{Stack, git, wait_for_turn_end};

#[test]
fn an_agent_is_told_where_to_keep_its_own_files_and_what_it_keeps_there_never_shows_as_a_change() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.unwrap());

    client.session_prompt(&session.id, "tool:echo what I tried > .context/tmp/notes.md", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let launched = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    for flag in ["--append-system-prompt", "--append-subagent-system-prompt"] {
        assert!(launched.contains(&format!("{flag} You're working in Roundhouse")), "{flag}: {launched}");
    }
    assert!(worktree.join(".context/tmp/notes.md").exists(), "the agent's command ran in its worktree");
    // Without what only this machine's own git ignores.
    let changes = ["-c", "core.excludesFile=/dev/null", "status", "--porcelain", "--untracked-files=all"];
    assert_eq!(git(&worktree, &changes), "");
}
