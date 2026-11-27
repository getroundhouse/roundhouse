//! Providers other than the local one, which a hub reaches through their adapters, and whose workers dial it.

use std::time::Duration;

use hub::Timers;
use protocol::event::EventBody;
use protocol::model::WorkspaceStatus;

use crate::support::{Stack, assistant_text, git, is_alive, read_all, started_pid, wait_for_turn_end, wait_until};

/// The hub waits for an adapter's hello, which mustn't keep clients waiting for the hub. Its limit here is far longer
/// than the test waits for the hub to serve, so a hub that waited for the hello first fails the wait, however fast or
/// slow the machine.
#[test]
fn a_hub_serves_while_a_providers_adapter_has_yet_to_say_hello() {
    let patient = Timers { adapter_hello_within: Duration::from_mins(10), ..Timers::FOR_TESTS };
    let mut stack = Stack::start_with(vec![patient.env()]);
    stack.kill_hub();
    // Reads what it's asked and never answers, until its stdin closes.
    let adapter = stack.path("silent-adapter");
    process::testing::write_program(&adapter, "#!/bin/sh\nwhile read -r line; do :; done\n").unwrap();
    let file = serde_json::json!({"default": "silent", "providers": [{"name": "silent", "program": adapter}]});
    std::fs::write(stack.path("home/providers.json"), file.to_string()).unwrap();

    stack.start_hub();
    let client = stack.client();

    assert!(client.repo_list().is_ok());
}

/// A provider whose workers run on a separate filesystem clones from the repository's remote address rather than using
/// the hub's checkout, so creating a workspace works even once the hub's checkout is gone.
#[test]
fn a_separate_provider_clones_from_the_repos_address_without_the_hubs_checkout() {
    let mut stack = Stack::start();
    std::fs::write(stack.repo().join("tracked.txt"), "from remote\n").unwrap();
    git(&stack.repo(), &["add", "tracked.txt"]);
    git(&stack.repo(), &["commit", "-qm", "add tracked file"]);
    let remote = stack.path("remote.git");
    git(&stack.repo(), &["clone", "--bare", stack.repo().to_str().unwrap(), remote.to_str().unwrap()]);
    git(&stack.repo(), &["remote", "add", "origin", &format!("file://{}", remote.display())]);
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    // One local workspace lets the hub's worker read the checkout's remote address onto the record.
    let local_ws = client.workspace_create(&repo.id, Some("local-first".into()), None, None).unwrap();
    wait_until("the repository's address to be read", || {
        client.repo_list().unwrap().into_iter().any(|r| r.id == repo.id && r.origin.url().is_some())
    });
    client.workspace_archive(&local_ws.id, false).unwrap();

    // Switch default provider to a separate-filesystem adapter and remove the hub's checkout entirely.
    stack.make_through_separate_adapter();
    std::fs::remove_dir_all(stack.repo()).unwrap();

    let client = stack.client();
    let ws = client.workspace_create(&repo.id, Some("separate-one".into()), None, None).unwrap();
    assert_eq!(ws.status, WorkspaceStatus::Ready);
    assert_eq!(ws.provider.as_str(), "recording");
    assert_eq!(
        std::fs::read_to_string(std::path::Path::new(ws.path.as_deref().unwrap()).join("tracked.txt")).unwrap(),
        "from remote\n"
    );
}

/// An ephemeral provider destroys its instance and repository clone on archive, while the hub keeps unpushed commits
/// in a bundle and restores them into a fresh clone on restore.
#[test]
fn an_ephemeral_provider_keeps_unpushed_commits_across_archive_and_restore() {
    let mut stack = Stack::start();
    let remote = stack.path("remote.git");
    git(&stack.repo(), &["clone", "--bare", stack.repo().to_str().unwrap(), remote.to_str().unwrap()]);
    git(&stack.repo(), &["remote", "add", "origin", &format!("file://{}", remote.display())]);
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    let local_ws = client.workspace_create(&repo.id, Some("local-first".into()), None, None).unwrap();
    wait_until("the repository's address to be read", || {
        client.repo_list().unwrap().into_iter().any(|r| r.id == repo.id && r.origin.url().is_some())
    });
    client.workspace_archive(&local_ws.id, false).unwrap();

    stack.make_through_ephemeral_adapter();
    let client = stack.client();
    let ws = client.workspace_create(&repo.id, Some("ephemeral-one".into()), None, None).unwrap();
    let wt = std::path::PathBuf::from(ws.path.as_deref().unwrap());
    std::fs::write(wt.join("unpushed.txt"), "kept in hub bundle\n").unwrap();
    git(&wt, &["add", "unpushed.txt"]);
    git(&wt, &["commit", "-qm", "unpushed work"]);
    let expected_head = git(&wt, &["rev-parse", "HEAD"]);

    let archived = client.workspace_archive(&ws.id, false).unwrap();
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    assert!(!stack.path("adapter-clones").exists(), "ephemeral destroy wiped the adapter's clones directory");
    let meta_path = stack.path(&format!("home/bundles/{}.json", ws.id));
    let bundle_path = stack.path(&format!("home/bundles/{}.bundle", ws.id));
    assert!(meta_path.exists(), "hub stored the archived metadata");
    assert!(bundle_path.exists(), "hub stored the raw archived commits bundle");

    let restored = client.workspace_restore(&ws.id).unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert!(!meta_path.exists(), "hub removed the metadata once Ready was committed");
    assert!(!bundle_path.exists(), "hub removed the raw bundle once Ready was committed");
    let restored_wt = std::path::PathBuf::from(restored.path.as_deref().unwrap());
    assert_eq!(git(&restored_wt, &["rev-parse", "HEAD"]), expected_head);
    assert_eq!(std::fs::read_to_string(restored_wt.join("unpushed.txt")).unwrap(), "kept in hub bundle\n");
}

/// So an adapter puts the hub's own build of the worker on a machine other than the hub's, as the provider protocol's
/// `Start` asks, from the builds the providers file names.
#[test]
fn a_hub_gives_an_adapter_the_worker_builds_its_providers_file_names() {
    let mut stack = Stack::start();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let worker = stack.path("linux-roundhoused");
    std::fs::write(&worker, "a linux worker").unwrap();

    stack.make_through_an_adapter_given(
        &serde_json::json!([{"target": "aarch64-unknown-linux-gnu", "path": worker.to_str().unwrap()}]),
    );
    let client = stack.client();
    client.workspace_create(&repo.id, Some("given".into()), None, None).unwrap();

    let given = protocol::provider::WorkerBinary {
        target: "aarch64-unknown-linux-gnu".into(),
        path: worker.to_string_lossy().into_owned(),
        sha256: "d9b8a662b18f6103eb2f4eae615868208ea5b44b00ab571fc8ad6582480597e2".into(),
    };
    assert_eq!(stack.workers_started(), [vec![given]]);
}

/// So a hub restarting for an update leaves an adapter's workers at work, as it does the local provider's: each dials
/// the next hub, which admits it by its key and carries on with its agent.
#[test]
fn an_adapters_worker_dials_the_next_hub_after_a_restart_and_its_agent_carries_on() {
    let mut stack = Stack::start();
    stack.make_through_an_adapter();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    assert_eq!(workspace.provider.as_str(), "recording");
    let (go, finished) = (stack.path("go"), stack.path("finished"));
    let script = format!("say:before; hang-until:{}; say:after; tool:touch {}", go.display(), finished.display());
    client.session_prompt(&session.id, &script, None).unwrap();
    wait_until("the agent to start its tool", || assistant_text(&read_all(&client, &session.id)) == "before");
    let agent = started_pid(&read_all(&client, &session.id)).unwrap();
    drop(client);

    crate::carrying_on::restart(&mut stack);
    assert!(is_alive(agent), "the restart left the agent running");
    std::fs::write(&go, "").unwrap();
    wait_until("the agent to run its last call", || finished.exists());
    stack.start_hub();

    let events = wait_for_turn_end(&stack.client(), &session.id, 1);
    assert_eq!(assistant_text(&events), "beforeafter", "{events:#?}");
    let started = events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the same agent carried on: {events:#?}");
    assert_eq!(stack.tickets_given().len(), 1, "the worker dialed the next hub without a ticket of its own");
}

/// A ticket admits one worker, once, so a second worker given the ticket an adapter was given is refused, and says so
/// as it exits, while the first serves on.
#[test]
fn a_ticket_an_adapter_was_given_admits_no_second_worker() {
    let mut stack = Stack::start();
    stack.make_through_an_adapter();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let (_, ticket) = stack.tickets_given().pop().unwrap();

    let second = process::testing::WholeEnvironment::for_a_test(&stack.path("second-home"))
        .command(env!("CARGO_BIN_EXE_roundhoused"))
        .args(["worker", "--ticket", &ticket, "--workspaces-root"])
        .arg(stack.path("second-workspaces"))
        .arg("--state-dir")
        .arg(stack.path("second-instance"))
        .arg("--clones-dir")
        .arg(stack.path("second-clones"))
        .output()
        .unwrap();
    client.session_prompt(&session.id, "say:still here", None).unwrap();

    assert!(!second.status.success(), "{second:?}");
    let said = String::from_utf8_lossy(&second.stderr);
    assert!(said.contains("the hub refused this worker"), "{said}");
    assert_eq!(assistant_text(&wait_for_turn_end(&client, &session.id, 1)), "still here");
}

/// So a worker that dials its hub is sent the hub's tool servers as one the hub runs itself is, and its agent calls
/// one with the person's key.
#[test]
fn an_adapters_worker_that_dials_is_sent_the_hubs_tool_servers() {
    use crate::tool_servers::{CALL, KEY, Took, add, stand_in};

    let (url, heard) = stand_in();
    let mut stack = Stack::start();
    stack.make_through_an_adapter();
    let client = stack.client();
    add(&client, url);
    let (workspace, chat) = stack.workspace(&client);

    let answered = crate::support::calls::run(&client, &chat.id, CALL, 1);

    assert_eq!(workspace.provider.as_str(), "recording");
    assert_eq!(answered, [("Found issue 7".to_string(), false)]);
    let took = Took { method: "tools/call".into(), key: KEY.into(), path: "/mcp".into() };
    assert_eq!(heard.try_iter().last(), Some(took), "it took the key from the worker");
}
