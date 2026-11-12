use std::path::PathBuf;

use protocol::frame;
use protocol::provider::{PROVIDER_PROTOCOL_VERSION, ProviderFrame, ProviderOutcome, ProviderResponse};
use providers::local::{LocalConfig, LocalProvider};
use providers::{Capabilities, ProviderError};

use super::*;

fn local(dir: &Path) -> Arc<dyn Provider> {
    Arc::new(LocalProvider::new(LocalConfig {
        worker_exe: dir.join("roundhoused"),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        shell: Some("/bin/sh".to_string()),
        path: None,
    }))
}

/// A script that says hello as an adapter called `name` would, then waits for its stdin to close.
fn adapter(dir: &Path, name: &str) -> PathBuf {
    let hello = ProviderResponse::Hello {
        protocol_version: PROVIDER_PROTOCOL_VERSION,
        adapter: name.into(),
        build: "test".into(),
        capabilities: Capabilities::default(),
    };
    let reply = frame::encode(&ProviderFrame::Reply { id: 1, outcome: ProviderOutcome::Response(hello) }).unwrap();
    let path = dir.join(name);
    let reply = String::from_utf8(reply).unwrap();
    std::fs::write(&path, format!("#!/bin/sh\nread -r hello\nprintf '%s' '{reply}'\ncat > /dev/null\n")).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    path
}

fn loaded(dir: &Path, file: Option<String>) -> ProviderSet {
    let path = dir.join("providers.json");
    if let Some(file) = file {
        std::fs::write(&path, file).unwrap();
    }
    ProviderSet::load(local(dir), &path, &"h".into())
}

fn default_name(set: &ProviderSet) -> Result<String, ApiError> {
    set.default_provider().map(|(name, provider)| {
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
