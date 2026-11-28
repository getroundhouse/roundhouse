use std::path::PathBuf;

use protocol::frame;
use protocol::provider::{HelloReply, PROVIDER_PROTOCOL_VERSION, ProviderFrame, ProviderOutcome, ProviderResponse};
use providers::local::{LocalConfig, LocalProvider};
use providers::{Capabilities, ProviderError, WorkerBinary};

use super::*;
use crate::Timers;
use crate::machine_environment::MachineEnvironment;

fn local(dir: &Path) -> Arc<dyn Provider> {
    Arc::new(LocalProvider::new(LocalConfig {
        worker_exe: dir.join("roundhoused"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        clones_dir: dir.join("clones"),
        worker_logs_dir: dir.join("worker-logs"),
        environment: process::Environment::NONE,
        dials: false,
        hub_address_file: None,
    }))
}

/// A script that says hello as an adapter called `name` would, then waits for its stdin to close.
fn adapter(dir: &Path, name: &str) -> PathBuf {
    let hello = ProviderResponse::Hello(HelloReply {
        protocol_version: PROVIDER_PROTOCOL_VERSION,
        adapter: name.into(),
        build: "test".into(),
        capabilities: Capabilities::default(),
        environment: providers::WorkerEnvironment::default(),
    });
    let reply = frame::encode(&ProviderFrame::Reply { id: 1, outcome: ProviderOutcome::Response(hello) }).unwrap();
    let path = dir.join(name);
    let reply = String::from_utf8(reply).unwrap();
    process::testing::write_program(
        &path,
        format!("#!/bin/sh\nread -r hello\nprintf '%s' '{reply}'\ncat > /dev/null\n"),
    )
    .unwrap();
    path
}

fn loaded(dir: &Path, file: Option<String>) -> ProviderSet {
    let path = dir.join("providers.json");
    if let Some(file) = file {
        std::fs::write(&path, file).unwrap();
    }
    let (within, machine) = (Timers::DEFAULT.adapter_hello_within, MachineEnvironment::only(&[]));
    ProviderSet::load(local(dir), &path, &"h".into(), Build::UNSTAMPED, within, &machine)
}

fn default_name(set: &ProviderSet) -> Result<String, ApiError> {
    set.for_new_workspace().map(|(name, provider)| {
        assert_eq!(provider.name(), name.as_str(), "the provider it names");
        name.to_string()
    })
}

#[tokio::test]
async fn without_a_file_there_is_only_the_local_provider() {
    let dir = tempfile::tempdir().unwrap();
    let set = loaded(dir.path(), None);
    assert_eq!(default_name(&set).unwrap(), "local");
    assert!(set.get(&"local".into()).is_some());
}

/// Roundhouse sets up GitHub on each provider the file names unless its entry says not to, and never on the local
/// provider's machine, which is the person's own, whether there's a file or not.
#[tokio::test]
async fn only_the_local_provider_and_one_whose_entry_says_so_are_left_to_their_machines() {
    let dir = tempfile::tempdir().unwrap();
    let program = adapter(dir.path(), "vm");
    let file = serde_json::json!({"providers": [
        {"name": "vms", "program": program},
        {"name": "keyed", "program": program, "set_up_github": false},
    ]});
    let set = loaded(dir.path(), Some(file.to_string()));

    assert_eq!(set.left_to_machine(), ["keyed".into(), "local".into()]);
    assert!(!set.leaves_github_to_machine(&"vms".into()));
    assert!(!set.leaves_github_to_machine(&"gone".into()), "one the hub doesn't know");
    let without = tempfile::tempdir().unwrap();
    assert_eq!(loaded(without.path(), None).left_to_machine(), ["local".into()], "without a file");
}

#[tokio::test]
async fn new_workspaces_go_to_the_provider_the_file_makes_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let program = adapter(dir.path(), "vm");
    let file = serde_json::json!({"default": "vms", "providers": [{"name": "vms", "program": program}]});
    let set = loaded(dir.path(), Some(file.to_string()));
    assert_eq!(default_name(&set).unwrap(), "vms", "named as the file names it");
    assert!(set.get(&"local".into()).is_some(), "the local provider is still there for workspaces made on it");
    set.say_hello().await;
    assert_eq!(set.adapters[0].name(), "vms");
    set.shut_down().await;
}

#[tokio::test]
async fn a_provider_whose_adapter_wont_start_is_kept_and_fails_until_it_does() {
    let dir = tempfile::tempdir().unwrap();
    let file =
        serde_json::json!({"default": "vms", "providers": [{"name": "vms", "program": dir.path().join("none")}]});
    let set = loaded(dir.path(), Some(file.to_string()));
    set.say_hello().await;

    assert_eq!(default_name(&set).unwrap(), "vms", "new workspaces don't go local instead");
    let listed = set.get(&"vms".into()).unwrap().list().await;
    assert!(matches!(listed, Err(ProviderError::Io { .. })), "{listed:?}");
}

#[tokio::test]
async fn a_file_that_doesnt_parse_leaves_no_provider_for_new_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let set = loaded(dir.path(), Some(r#"{"default": "vms", "providers": [{"nme": "vms"}]}"#.into()));
    let refused = default_name(&set).unwrap_err();
    assert_eq!(refused.code, ErrorCode::Unavailable);
    assert!(refused.message.contains("didn't parse"), "{}", refused.message);
    assert!(set.get(&"local".into()).is_some(), "local workspaces still work");
}

#[tokio::test]
async fn a_default_that_isnt_set_up_leaves_no_provider_for_new_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let set = loaded(dir.path(), Some(r#"{"default": "vms"}"#.into()));
    assert_eq!(default_name(&set).unwrap_err().code, ErrorCode::Unavailable);
}

/// So an adapter can tell whether an instance holds the worker already. One the hub can't read is left out, so an
/// adapter that needs it says the hub has none for its machine.
#[tokio::test]
async fn the_file_names_the_worker_builds_for_other_machines_with_their_checksums() {
    let dir = tempfile::tempdir().unwrap();
    let worker = dir.path().join("linux-roundhoused");
    std::fs::write(&worker, "a linux worker").unwrap();
    let file = serde_json::json!({"workers": [
        {"target": "aarch64-unknown-linux-gnu", "path": worker},
        {"target": "x86_64-unknown-linux-gnu", "path": dir.path().join("missing")},
    ]});

    let set = loaded(dir.path(), Some(file.to_string()));

    let expected = WorkerBinary {
        target: "aarch64-unknown-linux-gnu".into(),
        path: worker.to_string_lossy().into_owned(),
        sha256: "d9b8a662b18f6103eb2f4eae615868208ea5b44b00ab571fc8ad6582480597e2".into(),
    };
    assert_eq!(set.workers(), [expected]);
    assert!(loaded(tempfile::tempdir().unwrap().path(), None).workers().is_empty(), "without a file");
}
