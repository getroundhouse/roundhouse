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
    use std::os::unix::fs::PermissionsExt;

    let patient = Timers { adapter_hello_within: Duration::from_mins(10), ..Timers::FOR_TESTS };
    let mut stack = Stack::start_with(vec![patient.env()]);
    stack.kill_hub();
    // Reads what it's asked and never answers, until its stdin closes.
    let adapter = stack.path("silent-adapter");
    std::fs::write(&adapter, "#!/bin/sh\nwhile read -r line; do :; done\n").unwrap();
    std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o755)).unwrap();
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
