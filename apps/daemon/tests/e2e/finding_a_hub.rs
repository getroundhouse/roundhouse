//! Finding a hub.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use client_core::Client;
use protocol::model::{AgentKind, WorkspaceStatus};

use crate::support::{Stack, commit_settings, terminate, wait_for_setup, wait_for_turn_end, wait_until};

/// The desktop app starts a hub from a background thread with every signal blocked, and that carries
/// across exec. Blocking `SIGCHLD` alone is enough to make the first `git` hang.
#[test]
fn a_hub_started_with_every_signal_blocked_still_works() {
    use std::os::unix::process::CommandExt as _;

    let mut stack = Stack::start();
    stack.kill_hub();
    let mut command = stack.command();
    // SAFETY: both are async-signal-safe, and the set is initialized by the first.
    unsafe {
        command.pre_exec(|| {
            let mut all = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            libc::sigfillset(all.as_mut_ptr());
            libc::pthread_sigmask(libc::SIG_SETMASK, all.as_ptr(), std::ptr::null_mut());
            Ok(())
        });
    }
    stack.hub = Some(command.spawn().unwrap());
    wait_until("the hub to accept connections", || Client::connect(&stack.socket(), "test").is_ok());
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    // On its own thread, because what this checks for is a hang.
    let (created, result) = std::sync::mpsc::channel();
    let creating = client.clone();
    std::thread::spawn(move || {
        let _ = created.send(creating.workspace_create(&repo.id, None, None));
    });
    let workspace = result.recv_timeout(Duration::from_secs(20)).expect("creating a workspace hung").unwrap();
    assert_eq!(workspace.status, WorkspaceStatus::Ready);

    // An agent started under it works too, and stopping it by signal still does.
    let session =
        client.session_create(&workspace.id, AgentKind::Claude, None, Some("bypassPermissions".into())).unwrap();
    client.session_prompt(&session.id, "tool:echo done > done.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    assert!(PathBuf::from(workspace.path.unwrap()).join("done.txt").exists());
    drop(client);
    let mut hub = stack.hub.take().unwrap();
    terminate(hub.id());
    wait_until("the hub to stop on SIGTERM", || !matches!(hub.try_wait(), Ok(None)));
}

/// The code in a workspace can be another build of Trestle, whose commands would otherwise reach this hub.
#[test]
fn nothing_in_a_workspace_inherits_which_hub_this_is() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"env > setup-env.txt\"\n");
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    wait_for_setup(&client, &workspace.id);

    client.session_prompt(&session.id, "tool:env > agent-env.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let worktree = PathBuf::from(workspace.path.unwrap());
    for file in ["setup-env.txt", "agent-env.txt"] {
        let env = std::fs::read_to_string(worktree.join(file)).unwrap();
        assert!(env.lines().any(|line| line.starts_with("PATH=")), "{file} isn't an environment: {env}");
        let leaked: Vec<_> = env.lines().filter(|line| line.starts_with("TRESTLE_")).collect();
        assert!(leaked.is_empty(), "{file}: {leaked:?}");
    }
}

#[test]
fn a_hub_that_drops_the_connection_before_reading_it_is_replaced() {
    replaces_a_hub_that_drops_the_connection(false);
}

#[test]
fn a_hub_that_drops_the_connection_after_reading_it_is_replaced() {
    replaces_a_hub_that_drops_the_connection(true);
}

/// The client finds out differently depending on how far it got: a failed write, or no reply.
fn replaces_a_hub_that_drops_the_connection(reads_hello_first: bool) {
    let mut stack = Stack::start();
    stack.kill_hub();
    let socket = stack.socket();
    let _ = std::fs::remove_file(&socket);
    let dying = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let path = socket.clone();
    let hub_going_away = std::thread::spawn(move || {
        let (connection, _) = dying.accept().unwrap();
        if reads_hello_first {
            let mut hello = String::new();
            std::io::BufRead::read_line(&mut std::io::BufReader::new(&connection), &mut hello).unwrap();
        }
        // In this order, so the socket has gone before the client finds out and starts a real hub.
        drop(dying);
        std::fs::remove_file(path).unwrap();
        drop(connection);
    });

    let home = stack.path("home");
    let workspaces = stack.path("workspaces");
    let paths = protocol::paths::resolve(|name| match name {
        protocol::paths::ENV_HOME => Some(home.to_str().unwrap().to_string()),
        protocol::paths::ENV_WORKSPACES => Some(workspaces.to_str().unwrap().to_string()),
        _ => None,
    })
    .unwrap();
    let client = Client::connect_or_start(&paths, Path::new(env!("CARGO_BIN_EXE_trestled")), "test").unwrap();
    hub_going_away.join().unwrap();
    assert!(client.hub_status().is_ok());
    client.hub_shutdown().unwrap();
}
