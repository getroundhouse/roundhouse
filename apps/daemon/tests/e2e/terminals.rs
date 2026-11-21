//! A workspace's terminal through a real hub and a real worker: a shell in its worktree, what's typed into it and what
//! it shows, and how long it lasts.

#![cfg(test)]

use std::time::{Duration, Instant};

use client_core::{Client, OpenTerminal};
use process::ProcessRecord;
use protocol::model::{TerminalId, WorkspaceId};
use protocol::paths::AGENTS_DIR;
use protocol::terminal::{TerminalSize, Typed};

use crate::carrying_on::restart;
use crate::support::{self, Stack, is_alive, parent_of, signal, wait_until};

/// Wide enough that the worktree's path fits on one row.
const SIZE: TerminalSize = TerminalSize { cols: 400, rows: 24 };

/// A stack whose hubs start each terminal's shell as `/bin/sh`, so it's the same shell for everyone, rather than the login
/// shell that says nothing, which every other stack's hubs are given.
fn stack() -> Stack {
    Stack::start_with(vec![("SHELL", "/bin/sh".to_string())])
}

/// What `open` shows, as text, once it includes `wanted`.
fn shown_until(open: &OpenTerminal, wanted: &str) -> String {
    shown_when(open, wanted, |shown| shown.contains(wanted))
}

/// What `open` shows, as text, once `done` says it's all there.
fn shown_when(open: &OpenTerminal, what: &str, done: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut shown = String::new();
    while !done(&shown) {
        if let Some(frame) = open.take_frame() {
            shown = frame.rows.iter().map(|row| row.text()).collect::<Vec<_>>().join("\n");
            continue;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(open.changed().recv_timeout(left).is_ok(), "{what} never came, after {shown:?}");
    }
    shown
}

/// Types `typed` into `open` straight to the hub, as text the shell reads as it is, rather than through the open
/// terminal, which sends nothing for a key before its first picture.
fn type_in(client: &Client, open: &OpenTerminal, typed: &str) {
    client.terminal_typed(open.id(), Typed::Text { text: typed.to_owned() }).unwrap();
}

/// The process ID of the shell in `open`, which it prints as asked. What's typed is shown too, which the quotes keep
/// from looking like what it prints.
fn shell_pid(client: &Client, open: &OpenTerminal) -> u32 {
    type_in(client, open, "echo pi''d:$$:\r");
    printed_pid(open, "pid:")
}

/// The process ID that a program in `open` printed between `label` and a `:`.
fn printed_pid(open: &OpenTerminal, label: &str) -> u32 {
    let printed = |shown: &str| {
        let (_, after) = shown.rsplit_once(label)?;
        let (pid, _) = after.split_once(':')?;
        pid.parse().ok()
    };
    printed(&shown_when(open, label, |shown| printed(shown).is_some())).unwrap()
}

/// The group the worker recorded as the job terminal `t` runs in front, as the provider reads it.
fn recorded_in_front(stack: &Stack) -> Option<u32> {
    let instances = std::fs::read_dir(stack.paths().instances_dir).ok()?;
    let record = |dir: std::path::PathBuf| ProcessRecord::read(&dir.join(AGENTS_DIR).join("terminal-t.front.pid"));
    instances.flatten().find_map(|instance| record(instance.path())).map(|record| record.pid)
}

fn open_terminal(client: &Client, workspace: &WorkspaceId) -> OpenTerminal {
    let id = TerminalId::from("t");
    client.terminal_open(workspace, &id, SIZE).unwrap();
    let open = OpenTerminal::start(client, &id);
    // One that can't be followed says why at once, rather than once the wait gives up. A follow drops an error that
    // says the connection went, which the client says instead.
    wait_until("the terminal to be followed", || {
        let (errors, ended, gone) = (open.take_errors(), open.ended(), client.is_disconnected());
        assert!(
            errors.is_empty() && !ended && !gone,
            "it couldn't be followed: ended {ended}, hub gone {gone}, {errors:?}"
        );
        open.connected()
    });
    open
}

fn terminals_open(client: &Client) -> usize {
    client.records_list().unwrap().terminals.len()
}

#[test]
fn a_terminal_runs_a_shell_in_its_worktree_until_it_is_closed() {
    let stack = stack();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let worktree = support::workspace(&client, &workspace.id).path.unwrap();

    let open = open_terminal(&client, &workspace.id);
    type_in(&client, &open, "pwd -P; echo is-$((6*7))\r");
    let shown = shown_until(&open, "is-42");
    let pid = shell_pid(&client, &open);

    let worktree = std::fs::canonicalize(worktree).unwrap();
    assert!(shown.contains(worktree.to_str().unwrap()), "{shown:?}");
    assert_eq!(terminals_open(&client), 1);
    client.terminal_close(open.id()).unwrap();
    wait_until("the terminal to end", || open.ended());
    wait_until("its shell to end", || !is_alive(pid));
    assert_eq!(terminals_open(&client), 0);
}

/// As quitting the app does, and restarting the hub for an update, so neither ends what's running in it.
#[test]
fn a_terminal_outlasts_its_client_and_its_hub() {
    let mut stack = stack();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let open = open_terminal(&client, &workspace.id);
    type_in(&client, &open, "echo before-$((6*7))\r");
    shown_until(&open, "before-42");
    let pid = shell_pid(&client, &open);
    // A restart waits for the workspace's setup to finish, and opening a terminal doesn't, so this does first.
    support::wait_for_setup(&client, &workspace.id);
    drop((open, client));

    restart(&mut stack);
    stack.start_hub();
    let client = stack.client();

    assert!(is_alive(pid), "the restart left the shell running");
    assert_eq!(terminals_open(&client), 1, "the next hub carries on with it");
    let open = OpenTerminal::start(&client, &"t".into());
    // What it shows, which is what it showed.
    shown_until(&open, "before-42");
    type_in(&client, &open, "echo after-$((7*8))\r");
    shown_until(&open, "after-56");
}

/// A job a terminal runs in front can outlast its shell, as one that ignores being hung up on does, so a worker that's
/// killed, and can't stop it, leaves it recorded, and the provider stops it once the hub hears the worker has gone.
#[test]
fn the_job_a_killed_workers_terminal_ran_in_front_is_stopped() {
    let stack = stack();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let open = open_terminal(&client, &workspace.id);
    let shell = shell_pid(&client, &open);
    // `sleep` keeps the hangup ignored, and ends by itself, should the test fail before it's stopped.
    type_in(&client, &open, "sh -c 'trap \"\" HUP; echo jo''b:$$:; exec sleep 300'\r");
    let job = printed_pid(&open, "job:");
    wait_until("the job to be recorded", || recorded_in_front(&stack) == Some(job));

    // The shell's parent is the worker, which runs it in the PTY.
    signal(parent_of(shell), libc::SIGKILL);

    wait_until("the job to be stopped", || !is_alive(job));
}
