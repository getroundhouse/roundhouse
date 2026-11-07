//! Finding a hub.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use client_core::{Client, HubAdmin};
use protocol::model::{AgentKind, WorkspaceStatus};

use crate::support::{
    Stack, commit_settings, install_executable, terminate, wait_for_setup, wait_for_turn_end, wait_until,
};

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

/// Replacing a hub depends on this. If stopping returned before the old hub let go of its lock, a new one
/// could exit straight away, and nothing would start another.
#[test]
fn stopping_a_hub_returns_once_it_has_let_go_of_its_lock() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    drop(client);

    HubAdmin::connect(&stack.socket()).unwrap().stop().unwrap();

    // Not left to starting a new hub, as a client that gets there too soon waits out the old one anyway.
    let lock = std::fs::File::open(stack.path("home").join("hub.lock")).unwrap();
    lock.try_lock().expect("the hub should have let go of its lock");
    stack.hub.take().unwrap().wait().unwrap();
}

#[test]
fn a_client_can_start_a_new_hub_as_soon_as_stopping_the_last_one_returns() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:first", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    drop(client);
    let first = stack.hub_pid();
    let paths = protocol::paths::resolve(|name| match name {
        protocol::paths::ENV_HOME => Some(stack.path("home").to_str().unwrap().to_string()),
        protocol::paths::ENV_WORKSPACES => Some(stack.path("workspaces").to_str().unwrap().to_string()),
        _ => None,
    })
    .unwrap();

    HubAdmin::connect(&stack.socket()).unwrap().stop().unwrap();

    let daemon = Path::new(env!("CARGO_BIN_EXE_trestled"));
    let client = Client::connect_or_start(&paths, daemon, "test").unwrap();
    let status = client.hub_status().unwrap();
    // First, because nothing else would stop this hub if an assertion failed.
    client.hub_shutdown().unwrap();
    stack.hub.take().unwrap().wait().unwrap();
    assert_ne!(status.pid, first);
    assert!(status.on_demand, "a client started this one because it needed it");
}

/// Installing a new version moves the `current` link, and a running hub stays on the one it came from.
#[test]
fn a_hub_started_through_a_link_stays_on_its_version_when_the_link_moves() {
    use std::os::unix::fs::symlink;

    let mut stack = Stack::start();
    stack.kill_hub();
    let (old, new, current) = (stack.path("old"), stack.path("new"), stack.path("current"));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&new).unwrap();
    install_executable(Path::new(env!("CARGO_BIN_EXE_trestled")), &old.join("trestled"));
    std::fs::write(stack.path("broken"), "#!/bin/sh\nexit 1\n").unwrap();
    install_executable(&stack.path("broken"), &new.join("trestled"));
    symlink(&old, &current).unwrap();
    stack.hub = Some(stack.command_at(&current.join("trestled")).spawn().unwrap());
    wait_until("the hub to accept connections", || Client::connect(&stack.socket(), "test").is_ok());

    std::fs::remove_file(&current).unwrap();
    symlink(&new, &current).unwrap();

    // This is the first thing to need a worker, so it would start the broken one.
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "say:still here", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
}

/// The code in a workspace can be another build of Trestle, whose commands would otherwise reach this hub.
#[test]
fn nothing_in_a_workspace_inherits_which_hub_this_is() {
    let stack = Stack::start();
    commit_settings(&stack, "[scripts]\nsetup = \"env > setup-env.txt\"\n");
    // Git runs this while the worker adds a worktree, and any command the worker runs can do the same.
    let hook = stack.repo().join(".git/hooks/post-checkout");
    std::fs::write(&hook, "#!/bin/sh\nenv > hook-env.txt\n").unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    wait_for_setup(&client, &workspace.id);

    client.session_prompt(&session.id, "tool:env > agent-env.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let worktree = PathBuf::from(workspace.path.unwrap());
    for file in ["setup-env.txt", "agent-env.txt", "hook-env.txt"] {
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
