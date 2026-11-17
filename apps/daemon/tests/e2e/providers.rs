//! Providers other than the local one, which a hub reaches through their adapters.

use std::time::{Duration, Instant};

use crate::support::Stack;

/// How long the hub waits for an adapter's hello, as `HELLO_TIMEOUT` in its `provider_link` says.
const HELLO_WITHIN: Duration = Duration::from_secs(10);

/// It waits up to [`HELLO_WITHIN`] for an adapter's hello, which mustn't keep clients waiting for the hub.
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
    // A hub that waited for the hello first can't answer before its wait ends, however fast the machine, so anything
    // sooner proves it didn't, and a slow machine fails this only by taking longer to start a hub than a test may take.
    assert!(started.elapsed() < HELLO_WITHIN, "took {:?}", started.elapsed());
}
