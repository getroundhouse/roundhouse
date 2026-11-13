//! What every agent is told about working in Roundhouse, and the `.context/` folder its instructions send it to.

#![cfg(test)]

use std::path::PathBuf;

use crate::support::{Stack, git, wait_for_turn_end};

#[test]
fn an_agent_is_told_where_to_keep_its_own_files_and_what_it_keeps_there_never_shows_as_a_change() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.unwrap());

    let script = "tool:mkdir .context/research && echo what I tried > .context/research/notes.md";
    client.session_prompt(&session.id, script, None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let launched = std::fs::read_to_string(stack.agent_state().join("last-args")).unwrap();
    for flag in ["--append-system-prompt", "--append-subagent-system-prompt"] {
        assert!(launched.contains(&format!("{flag} You're working in Roundhouse")), "{flag}: {launched}");
    }
    assert!(worktree.join(".context/research/notes.md").exists(), "the agent's command ran in its worktree");
    // Without what only this machine's own git ignores.
    let changes = ["-c", "core.excludesFile=/dev/null", "status", "--porcelain", "--untracked-files=all"];
    assert_eq!(git(&worktree, &changes), "");
}
