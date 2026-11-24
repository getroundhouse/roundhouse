//! Providers other than the local one, which a hub reaches through their adapters.

use std::time::Duration;

use hub::Timers;
use protocol::model::WorkspaceStatus;

use crate::support::{Stack, git, wait_until};

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
