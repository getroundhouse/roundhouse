//! A session's normal life, from the first prompt to the answer.

#![cfg(test)]

use std::path::PathBuf;
use std::process::Command;

use client_core::Client;
use protocol::event::EventBody;
use protocol::model::WorkspaceStatus;

use crate::support::{Stack, assert_gapless, assistant_text, git, read_all, wait_for_turn_end, wait_until};

#[test]
fn a_session_runs_in_a_real_worktree_with_the_worker_as_a_separate_process() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    assert_eq!(git(&worktree, &["branch", "--show-current"]), workspace.branch);
    assert_eq!(workspace.status, WorkspaceStatus::Ready);

    client.session_prompt(&session.id, "say:hello from the agent; tool:pwd && echo made > made.txt", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);
    assert_gapless(&events);
    assert_eq!(assistant_text(&events), "hello from the agent");

    // The agent ran in the worktree, not in the repository or the hub's directory.
    assert!(worktree.join("made.txt").exists());
    assert!(!stack.repo().join("made.txt").exists());
    let reported = events.iter().find_map(|e| match &e.body {
        EventBody::AgentInit { cwd, .. } => cwd.clone(),
        _ => None,
    });
    assert_eq!(reported.map(|c| PathBuf::from(c).canonicalize().unwrap()), Some(worktree.canonicalize().unwrap()));

    // hub -> worker -> agent, each its own process.
    let agent_pid = events.iter().find_map(|e| match &e.body {
        EventBody::AgentStarted { pid, .. } => Some(*pid),
        _ => None,
    });
    let agent_pid = agent_pid.expect("an AgentStarted event");
    let parent_of = |pid: u32| -> u32 {
        let out = Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    };
    let worker_pid = parent_of(agent_pid);
    assert_ne!(worker_pid, stack.hub_pid());
    assert_eq!(parent_of(worker_pid), stack.hub_pid());
}

#[test]
fn prompts_are_recorded_before_anything_else_and_retries_are_recorded_once() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let (first, duplicate) = client.session_prompt(&session.id, "say:once", Some("msg-1".into())).unwrap();
    assert!(!duplicate);
    assert_eq!(first, 1);
    let (again, duplicate) = client.session_prompt(&session.id, "say:once", Some("msg-1".into())).unwrap();
    assert!(duplicate);
    assert_eq!(again, first);

    let events = wait_for_turn_end(&client, &session.id, 1);
    let prompts = events.iter().filter(|e| matches!(e.body, EventBody::UserPrompt { .. })).count();
    assert_eq!(prompts, 1);
    assert_eq!(assistant_text(&events), "once");
    let ended = events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    assert_eq!(ended, 1, "the agent must only receive it once");
}

#[test]
fn a_prompt_sent_mid_turn_folds_in_and_the_session_stays_running_until_it_is_done() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "slow:10:first", None).unwrap();
    wait_until("the first turn to start", || {
        read_all(&client, &session.id).iter().any(|e| matches!(e.body, EventBody::AgentInit { .. }))
    });
    client.session_prompt(&session.id, "say:second", None).unwrap();

    let events = wait_for_turn_end(&client, &session.id, 1);
    let text = assistant_text(&events);
    assert!(text.ends_with("second"), "{text}");
    let ended = events.iter().filter(|e| matches!(e.body, EventBody::TurnEnded { .. })).count();
    assert_eq!(ended, 1, "a folded prompt has no result of its own");
}

#[test]
fn many_sessions_run_at_once_without_mixing_their_output() {
    let stack = Stack::start();
    let client = stack.client();
    let sessions: Vec<_> = (0..4)
        .map(|i| {
            let (workspace, session) = stack.workspace(&client);
            client.session_prompt(&session.id, &format!("slow:8:s{i}-; tool:pwd"), None).unwrap();
            (workspace, session, format!("s{i}-"))
        })
        .collect();
    for (_, session, marker) in &sessions {
        let events = wait_for_turn_end(&client, &session.id, 1);
        assert_gapless(&events);
        let text = assistant_text(&events);
        assert!(text.contains(marker.as_str()), "{text}");
        for (_, _, other) in &sessions {
            assert!(other == marker || !text.contains(other.as_str()), "{text}");
        }
    }
    assert_eq!(client.hub_status().unwrap().workers, 4);
}

#[test]
fn the_agent_does_not_inherit_a_parent_sessions_identity() {
    let mut stack = Stack::start();
    stack.kill_hub();
    let mut command = stack.command();
    command.env("CLAUDECODE", "1").env("CLAUDE_CODE_SESSION_ID", "parent").env("ANTHROPIC_API_KEY", "kept");
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub", || Client::connect(&stack.socket(), "test").is_ok());

    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client
        .session_prompt(
            &session.id,
            "env:CLAUDECODE; env:CLAUDE_CODE_SESSION_ID; env:ANTHROPIC_API_KEY; env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS",
            None,
        )
        .unwrap();
    let text = assistant_text(&wait_for_turn_end(&client, &session.id, 1));
    assert!(text.contains("CLAUDECODE=<unset>"), "{text}");
    assert!(text.contains("CLAUDE_CODE_SESSION_ID=<unset>"), "{text}");
    assert!(text.contains("ANTHROPIC_API_KEY=kept"), "the user's own login settings stay: {text}");
    assert!(text.contains("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1"), "{text}");
}

#[test]
fn raw_agent_lines_are_kept_so_the_log_can_be_rebuilt() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let (events, _) = client.session_read(&session.id, 1, 100, true).unwrap();
    let assistant = events.iter().find(|e| matches!(e.body, EventBody::Assistant { .. })).unwrap();
    let raw: serde_json::Value = serde_json::from_str(assistant.raw.as_ref().unwrap()).unwrap();
    assert_eq!(raw["message"]["content"][0]["text"], "hello");
    // The prompt was the hub's own record, so it has no raw line.
    assert!(events[0].raw.is_none());
}

#[test]
fn a_session_says_when_its_newest_turn_started() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let (seq, _) = client.session_prompt(&session.id, "say:hello", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 1);

    let prompt = events.iter().find(|e| e.seq == seq).unwrap();
    assert_eq!(client.session_get(&session.id).unwrap().turn_started_at_ms, Some(prompt.ts_ms));
}

#[test]
fn each_turn_ends_saying_when_it_started() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);

    let mut prompts = Vec::new();
    for turn in 1..=2 {
        let (seq, _) = client.session_prompt(&session.id, "say:hello", None).unwrap();
        prompts.push(seq);
        wait_for_turn_end(&client, &session.id, turn);
    }
    let events = read_all(&client, &session.id);

    let started: Vec<Option<i64>> =
        prompts.iter().map(|seq| events.iter().find(|e| e.seq == *seq).map(|prompt| prompt.ts_ms)).collect();
    let ends: Vec<Option<i64>> = events
        .iter()
        .filter_map(|e| match e.body {
            EventBody::TurnEnded { started_at_ms, .. } => Some(started_at_ms),
            _ => None,
        })
        .collect();
    assert_ne!(started[0], started[1], "the turns have to start apart to tell them apart");
    assert_eq!(ends, started);
}
