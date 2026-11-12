use std::path::PathBuf;

use protocol::client::ErrorCode;
use tempfile::TempDir;

use super::*;
use crate::cli::parse_list;

fn incus_at(program: PathBuf) -> Incus {
    Incus { program, remote: "lab".into(), project: "roundhouse".into() }
}

fn config() -> Config {
    Config {
        base_image: "rh-base".into(),
        target: "x86_64-unknown-linux-gnu".into(),
        user: 1002,
        group: 1003,
        home: "/home/roundhouse".into(),
        workspaces_root: "/home/roundhouse/workspaces".into(),
        state_dir: "/var/lib/roundhouse/instance".into(),
        worker: "/usr/local/bin/roundhoused".into(),
        env: vec![("ROUNDHOUSE_CLAUDE_BIN".into(), "/usr/local/bin/claude".into())],
    }
}

#[test]
fn a_vm_is_named_for_its_instance_in_what_incus_allows() {
    assert_eq!(vm_name(&"0D7C3A1E-5b2f_x".into()), "rh-0d7c3a1e-5b2f-x");
}

#[test]
fn the_worker_runs_as_its_user_with_no_terminal_to_mangle_its_stream() {
    let command = worker_command(&incus_at("/usr/bin/incus".into()), &config(), "rh-i");
    assert_eq!(command.program, "/usr/bin/incus");
    let path = "PATH=/home/roundhouse/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    #[rustfmt::skip]
    let expected = [
        "--project", "roundhouse", "exec", "lab:rh-i", "-T", "--user", "1002", "--group", "1003",
        "--cwd", "/home/roundhouse", "--env", "HOME=/home/roundhouse", "--env", path,
        "--env", "ROUNDHOUSE_CLAUDE_BIN=/usr/local/bin/claude",
        "--", "/usr/local/bin/roundhoused", "worker", "--stdio", "--workspaces-root", "/home/roundhouse/workspaces",
        "--state-dir", "/var/lib/roundhouse/instance",
    ];
    assert_eq!(command.args, expected);
}

#[test]
fn the_list_is_read_as_incus_prints_it() {
    // Trimmed from `incus list --format json` on Incus 7.6.
    let json = r#"[{"architecture":"x86_64","config":{"limits.cpu":"4","user.roundhouse.hub":"h",
        "user.roundhouse.instance":"i"},"name":"rh-i","status":"Running","status_code":103,"type":"virtual-machine"},
        {"name":"rh-base","status":"Stopped","config":{}},{"name":"rh-going","status":"Stopping","config":null},
        {"name":"rh-cold","status":"Frozen","config":{}}]"#;
    let listed = parse_list(json).unwrap();
    let states: Vec<_> = listed.iter().map(state).collect();
    assert_eq!(
        states,
        [InstanceState::Running, InstanceState::Stopped, InstanceState::Stopping, InstanceState::Unknown]
    );
    assert_eq!(listed[0].tag(HUB_KEY), Some("h"));
    assert_eq!(listed[2].config, None, "not yet known, rather than empty");
}

/// Stands in for `incus`. It writes down how it's called, lists what the next of `list-N.json` holds, or else
/// `list.json`, fails a copy with what `copy-fails` holds, prints `snapshots` for a list of snapshots, and exits from
/// the script that ends leftovers with what `end-status` holds.
struct FakeIncus {
    dir: TempDir,
}

impl FakeIncus {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().display();
        let script = format!(
            r#"#!/bin/sh
for arg in "$@"; do printf '%s\037' "$arg"; done >> "{d}/calls"; printf '\036' >> "{d}/calls"
case " $* " in
  *" snapshot list "*) cat "{d}/snapshots" 2>/dev/null || true ;;
  *" list "*)
    n=$(cat "{d}/lists-read" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "{d}/lists-read"
    if [ -e "{d}/list-$n.json" ]; then cat "{d}/list-$n.json"; else cat "{d}/list.json"; fi ;;
  *" copy "*) if [ -e "{d}/copy-fails" ]; then cat "{d}/copy-fails" >&2; exit 1; fi ;;
  *" sh -c "*) exit "$(cat "{d}/end-status" 2>/dev/null || echo 0)" ;;
esac
"#
        );
        let program = dir.path().join("incus");
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn provider(&self) -> IncusProvider {
        IncusProvider::new(incus_at(self.dir.path().join("incus")), "h".into(), config())
    }

    /// What `list` says each time it's asked, the last from then on.
    fn lists(&self, lists: &[&str]) {
        for (n, json) in lists.iter().enumerate() {
            std::fs::write(self.dir.path().join(format!("list-{}.json", n + 1)), json).unwrap();
        }
        std::fs::write(self.dir.path().join("list.json"), lists.last().unwrap()).unwrap();
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.dir.path().join(name), contents).unwrap();
    }

    /// How many times it was asked for the list of instances.
    fn lists_read(&self) -> u32 {
        std::fs::read_to_string(self.dir.path().join("lists-read")).map_or(0, |n| n.trim().parse().unwrap())
    }

    /// How it was called, but for listing instances, each after the project it's always given.
    fn calls(&self) -> Vec<Vec<String>> {
        let calls = std::fs::read_to_string(self.dir.path().join("calls")).unwrap_or_default();
        calls
            .split('\u{1e}')
            .filter(|call| !call.is_empty())
            .map(|call| call.split('\u{1f}').filter(|arg| !arg.is_empty()).map(String::from).collect::<Vec<_>>())
            .inspect(|args| assert_eq!(args[..2], ["--project", "roundhouse"], "{args:?}"))
            .map(|args| args[2..].to_vec())
            .filter(|args| args[0] != "list")
            .collect()
    }
}

fn vm(name: &str, status: &str) -> String {
    vm_of("h", name, status)
}

fn vm_of(hub: &str, name: &str, status: &str) -> String {
    format!(
        r#"{{"name":"{name}","status":"{status}","config":{{"user.roundhouse.hub":"{hub}","user.roundhouse.instance":"i"}}}}"#
    )
}

#[tokio::test]
async fn an_instance_is_copied_under_another_name_and_renamed_once_its_whole() {
    let incus = FakeIncus::new();
    // One cut short by an adapter that was killed.
    incus.lists(&[&format!("[{}]", vm("rh-i-making", "Stopped"))]);

    incus.provider().create(&"i".into(), None, None, "holly").await.unwrap();

    #[rustfmt::skip]
    let expected = [
        vec!["delete", "--force", "lab:rh-i-making"],
        vec![
            "copy", "lab:rh-base/base", "lab:rh-i-making", "--config", "user.roundhouse.hub=h",
            "--config", "user.roundhouse.instance=i", "--config", "user.roundhouse.name=holly",
            "--config", "user.roundhouse.base-image=false",
        ],
        vec!["move", "lab:rh-i-making", "lab:rh-i"],
    ];
    assert_eq!(incus.calls(), expected);
}

#[tokio::test]
async fn an_instance_thats_there_is_left_as_it_is() {
    let incus = FakeIncus::new();
    incus.lists(&[&format!("[{}]", vm("rh-i", "Stopped"))]);

    incus.provider().create(&"i".into(), None, None, "holly").await.unwrap();

    assert_eq!(incus.calls(), Vec::<Vec<String>>::new());
}

#[tokio::test]
async fn another_hubs_vm_of_the_same_name_is_refused_rather_than_taken() {
    let incus = FakeIncus::new();
    incus.lists(&[&format!("[{}]", vm_of("other", "rh-i", "Running"))]);

    let made = incus.provider().create(&"i".into(), None, None, "holly").await;

    let Err(ProviderError::Adapter(e)) = made else { panic!("{made:?}") };
    assert_eq!(e.code, ErrorCode::Conflict);
    assert_eq!(incus.calls(), Vec::<Vec<String>>::new());
}

#[tokio::test]
async fn another_hubs_instances_are_neither_listed_nor_touched() {
    let incus = FakeIncus::new();
    incus.lists(&[&format!("[{},{}]", vm_of("other", "rh-i", "Stopped"), vm_of("h", "rh-j", "Running"))]);
    // Named as the other hub's instance is, so that only its hub says whose it is.
    let provider = incus.provider();

    let listed = provider.list().await.unwrap();
    let started = provider.start(&"i".into(), &[]).await;
    provider.destroy(&"i".into()).await.unwrap();
    provider.stop(&"i".into()).await.unwrap();

    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].state, InstanceState::Running);
    assert!(matches!(started, Err(ProviderError::NotFound(_))), "{started:?}");
    assert_eq!(incus.calls(), Vec::<Vec<String>>::new());
}

#[tokio::test]
async fn a_project_out_of_room_is_a_provider_without_room() {
    let incus = FakeIncus::new();
    incus.lists(&["[]"]);
    let refusal =
        r#"Error: Failed creating instance record: Reached maximum aggregate value "48GiB" for "limits.memory""#;
    incus.write("copy-fails", refusal);

    let made = incus.provider().create(&"i".into(), None, None, "holly").await;

    let Err(ProviderError::Adapter(e)) = made else { panic!("{made:?}") };
    assert_eq!(e.code, ErrorCode::Unavailable);
    assert!(e.message.contains("48GiB"), "{}", e.message);
}

#[tokio::test]
async fn starting_one_thats_stopped_starts_it_then_waits_for_its_agent() {
    let incus = FakeIncus::new();
    incus.lists(&[&format!("[{}]", vm("rh-i", "Stopped"))]);

    incus.provider().start(&"i".into(), &[]).await.unwrap();

    assert_eq!(incus.calls(), [vec!["start", "lab:rh-i"], vec!["exec", "lab:rh-i", "--", "true"]]);
}

#[tokio::test]
async fn starting_one_thats_stopping_waits_for_it_to_stop_then_for_its_agent() {
    let incus = FakeIncus::new();
    // Listed without its config at first, as Incus lists one that's stopping.
    let stopping = r#"[{"name":"rh-i","status":"Stopping","config":null}]"#;
    incus.lists(&[stopping, &format!("[{}]", vm("rh-i", "Stopping")), &format!("[{}]", vm("rh-i", "Stopped"))]);

    incus.provider().start(&"i".into(), &[]).await.unwrap();

    assert_eq!(incus.lists_read(), 3, "started only once it was listed as stopped");
    assert_eq!(incus.calls(), [vec!["start", "lab:rh-i"], vec!["exec", "lab:rh-i", "--", "true"]]);
}

#[tokio::test]
async fn starting_one_thats_running_or_starting_only_waits_for_its_agent() {
    for status in ["Running", "Starting"] {
        let incus = FakeIncus::new();
        incus.lists(&[&format!("[{}]", vm("rh-i", status))]);

        incus.provider().start(&"i".into(), &[]).await.unwrap();

        assert_eq!(incus.calls(), [vec!["exec", "lab:rh-i", "--", "true"]], "{status}");
    }
}

#[tokio::test]
async fn stopping_ends_what_runs_as_the_worker_in_one_script_which_says_if_it_worked() {
    let incus = FakeIncus::new();
    incus.lists(&[&format!("[{}]", vm("rh-i", "Running"))]);
    let provider = incus.provider();

    provider.stop(&"i".into()).await.unwrap();
    incus.write("end-status", "3");
    let killed = provider.kill(&"i".into()).await;

    let script = |tenths| vec!["exec", "lab:rh-i", "--", "sh", "-c", END_USER_SCRIPT, "sh", "1002", tenths];
    // As the worker's user, for 10 s, then at once.
    assert_eq!(incus.calls(), [script("100"), script("0")]);
    assert!(matches!(killed, Err(ProviderError::Io { .. })), "something outlived SIGKILL: {killed:?}");
}

#[tokio::test]
async fn a_base_image_is_shut_down_cleanly_within_a_time_then_kept_as_a_snapshot_once() {
    let incus = FakeIncus::new();
    incus.lists(&[&format!("[{}]", vm("rh-i", "Running"))]);
    let provider = incus.provider();

    provider.make_base_image(&"i".into()).await.unwrap();
    incus.write("snapshots", "base,2026/11/11 10:00 UTC,,NO\n");
    provider.make_base_image(&"i".into()).await.unwrap();

    let listing = vec!["snapshot", "list", "lab:rh-i", "--format", "csv"];
    let mark = vec!["config", "set", "lab:rh-i", "user.roundhouse.base-image=true"];
    let stop = vec!["stop", "lab:rh-i", "--timeout", "120"];
    let expected = [
        stop.clone(),
        listing.clone(),
        vec!["snapshot", "create", "lab:rh-i", "base"],
        mark.clone(),
        stop,
        listing,
        mark,
    ];
    assert_eq!(incus.calls(), expected);
}
