//! Providers other than the local one, which a hub reaches through their adapters.

use std::time::{Duration, Instant};

use crate::support::Stack;

/// It waits up to 10 s for an adapter's hello, which mustn't keep clients waiting for the hub.
#[test]
fn a_hub_serves_while_a_providers_adapter_has_yet_to_say_hello() {
    use std::os::unix::fs::PermissionsExt;

    let mut stack = Stack::start();
    stack.kill_hub();
    // Reads what it's asked and never answers, until its stdin closes.
    let adapter = stack.path("silent-adapter");
    std::fs::write(&adapter, "#!/bin/sh\nwhile read -r line; do :; done\n").unwrap();
    std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = serde_json::json!({"default": "silent", "providers": [{"name": "silent", "program": adapter}]});
    std::fs::write(stack.path("home/providers.json"), file.to_string()).unwrap();
    let started = Instant::now();

    stack.start_hub();
    let client = stack.client();

    assert!(client.repo_list().is_ok());
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
}
