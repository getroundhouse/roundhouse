use super::*;
use std::os::unix::process::ExitStatusExt;

#[test]
fn the_path_survives_whatever_rc_files_print_around_it() {
    let output = format!("Welcome!\n{PATH_MARKER}/a:/b\n{PATH_MARKER}\nbye\n");
    assert_eq!(parse_marked_path(&output).as_deref(), Some("/a:/b"));
    assert_eq!(parse_marked_path("no markers"), None);
    assert_eq!(parse_marked_path(&format!("{PATH_MARKER}{PATH_MARKER}")), None);
}

#[test]
fn merging_keeps_login_entries_first_and_drops_duplicates() {
    assert_eq!(merge_paths(Some("/a:/b"), Some("/b:/c")).as_deref(), Some("/a:/b:/c"));
    assert_eq!(merge_paths(None, Some("/c")).as_deref(), Some("/c"));
    assert_eq!(merge_paths(None, None), None);
}

#[tokio::test]
async fn the_login_shell_really_reports_a_path() {
    // Runs the real shell, so this also covers rc files that print to stdout.
    let path = read_login_path(None).await;
    assert!(path.as_deref().is_some_and(|p| p.contains('/')), "got {path:?}");
}

#[tokio::test]
async fn reading_the_path_does_not_wait_for_a_stream_something_else_is_holding_open() {
    use tokio::io::AsyncWriteExt;

    let (mut writer, reader) = tokio::io::duplex(1024);
    writer.write_all(format!("noise {PATH_MARKER}/a:/b{PATH_MARKER}").as_bytes()).await.unwrap();
    // `writer` stays open, as it would with a background process started by an rc file.
    let out = tokio::time::timeout(Duration::from_secs(1), read_until_second_marker(reader)).await.unwrap();
    assert_eq!(out.as_deref().and_then(parse_marked_path).as_deref(), Some("/a:/b"));
}

/// What a killed worker left gets to stop by itself here: long enough for a test's own processes, which stop as soon
/// as they're asked, and short enough that a test of one that won't doesn't wait out the product's.
const GRACE: Duration = Duration::from_millis(500);

fn orphan(dir: &Path) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    std::process::Command::new("sleep").arg("100").process_group(0).current_dir(dir).spawn().unwrap()
}

fn record_agent(dir: &Path, name: &str, pid: u32, started: &str) -> PathBuf {
    std::fs::create_dir_all(dir.join("agents")).unwrap();
    let path = dir.join("agents").join(name);
    std::fs::write(&path, format!("{pid}\n{started}\n")).unwrap();
    path
}

#[tokio::test]
async fn a_recorded_orphan_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = orphan(dir.path());
    let started = process::start_time(child.id()).unwrap();
    let path = record_agent(dir.path(), "s.pid", child.id(), &started);
    // It's this test's own child, so it stays a zombie until reaped, and `ps` still lists it.
    // Reap it from another thread so the wait below sees it go.
    let reaper = std::thread::spawn(move || child.wait().unwrap());
    stop_orphans(dir.path(), GRACE).await;
    assert!(reaper.join().unwrap().signal().is_some(), "it should have been signalled");
    assert!(!path.exists());
}

/// Starts `script` as the leader of a group of its own, recorded as `name` in the instance's records, with a thread
/// that reaps it, so it doesn't stay a zombie, which a group counts as running.
fn recorded_group(dir: &Path, name: &str, script: &str) -> (u32, std::thread::JoinHandle<std::process::ExitStatus>) {
    use std::os::unix::process::CommandExt;
    let mut leader = std::process::Command::new("sh").args(["-c", script]).process_group(0).spawn().unwrap();
    let pid = leader.id();
    record_agent(dir, name, pid, &process::start_time(pid).unwrap());
    (pid, std::thread::spawn(move || leader.wait().unwrap()))
}

/// Waits for `path`, which a test's script makes once it has got as far as the test needs, saying `never` if it doesn't.
async fn until_there(path: &Path, never: &str) {
    waiting::task::until(Duration::from_secs(10), never, || path.exists()).await;
}

/// Whether anything is left of the group `leader` leads, which is killed if so, so a test that fails doesn't leave it
/// running for good. Its ID is its own while anything in it runs.
fn killed_if_left(leader: u32) -> bool {
    let left = process::group_running(leader);
    if left {
        process::signal_group(leader, process::Signal::Kill);
    }
    left
}

/// A group can outlast its leader, as a server a script started does when the script exits on SIGTERM, so what's left
/// of it is made to stop too.
#[tokio::test]
async fn what_a_recorded_group_leaves_once_its_leader_exits_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left");
    // The leader exits on SIGTERM, and what it started ignores it, which what that starts inherits.
    let script = format!("sh -c 'trap \"\" TERM; : > {}; while :; do sleep 0.05; done' & wait", left.display());
    let (leader, reaped) = recorded_group(dir.path(), "s.pid", &script);
    until_there(&left, "what the leader started never ran").await;

    stop_orphans(dir.path(), GRACE).await;
    let left_running = killed_if_left(leader);

    assert_eq!(reaped.join().unwrap().signal(), Some(libc::SIGTERM), "the leader stopped when asked");
    assert!(!left_running, "what it left was stopped too");
}

/// Each is asked at once, so they share one grace, rather than each waiting for the one before.
#[tokio::test]
async fn recorded_groups_that_ignore_being_asked_are_stopped_together() {
    let dir = tempfile::tempdir().unwrap();
    let stubborn = |ready: &Path| format!("trap '' TERM; : > {}; while :; do sleep 0.05; done", ready.display());
    let ready = [dir.path().join("a.ready"), dir.path().join("b.ready")];
    let (first, first_reaped) = recorded_group(dir.path(), "a.pid", &stubborn(&ready[0]));
    let (second, second_reaped) = recorded_group(dir.path(), "b.pid", &stubborn(&ready[1]));
    // A shell asked before it gets to its `trap` would stop when asked, as these mustn't.
    for ready in &ready {
        until_there(ready, "a group never got to ignoring being asked").await;
    }
    let started = std::time::Instant::now();

    stop_orphans(dir.path(), GRACE).await;
    let took = started.elapsed();
    let left = [first, second].map(killed_if_left);

    assert!(took < GRACE * 2, "took {took:?}");
    assert_eq!(left, [false, false], "each was stopped");
    for reaped in [first_reaped, second_reaped] {
        assert_eq!(reaped.join().unwrap().signal(), Some(libc::SIGKILL));
    }
}

#[tokio::test]
async fn a_pid_now_used_by_something_else_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = orphan(dir.path());
    let path = record_agent(dir.path(), "s.pid", child.id(), "Thu Jan  1 00:00:00 1970");
    stop_orphans(dir.path(), GRACE).await;
    assert!(child.try_wait().unwrap().is_none(), "this isn't the process that was recorded");
    assert!(!path.exists(), "the stale record still goes");
    child.kill().unwrap();
    child.wait().unwrap();
}

#[tokio::test]
async fn a_runs_other_files_go_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("agents")).unwrap();
    let tools = dir.path().join("agents/s-1.mcp.json");
    std::fs::write(&tools, r#"{"mcpServers":{}}"#).unwrap();

    stop_orphans(dir.path(), GRACE).await;

    assert!(!tools.exists(), "it holds the agent's token");
}

fn provider(dir: &Path) -> LocalProvider {
    LocalProvider::new(LocalConfig {
        worker_exe: "/bin/roundhoused".into(),
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        clones_dir: dir.join("clones"),
        shell: Some("/bin/sh".into()),
        path: Some("/opt/tools".into()),
    })
}

/// Records `child` as the worker serving the instance in `dir`, as a worker does once it has claimed it.
fn record_worker(dir: &Path, child: &std::process::Child) {
    ProcessRecord::write(&dir.join(WORKER_RECORD_FILE), child.id()).unwrap();
}

#[tokio::test]
async fn every_request_is_safe_to_repeat() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "i".into();

    for _ in 0..2 {
        provider.create(&id, None, None, "roundhouse-i").await.unwrap();
        provider.start(&id, &[]).await.unwrap();
    }
    let first = provider.connect(&id).await.unwrap();
    let second = provider.connect(&id).await.unwrap();
    for _ in 0..2 {
        provider.stop(&id).await.unwrap();
        provider.kill(&id).await.unwrap();
        provider.destroy(&id).await.unwrap();
    }

    assert_eq!(first, second);
    assert!(provider.list().await.unwrap().is_empty());
    assert!(matches!(provider.connect(&id).await, Err(ProviderError::NotFound(_))));
}

#[tokio::test]
async fn connect_answers_with_the_hubs_own_worker_for_the_instance() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "i".into();
    provider.create(&id, None, None, "roundhouse-i").await.unwrap();

    let command = provider.connect(&id).await.unwrap();

    let text = |path: PathBuf| path.to_string_lossy().into_owned();
    assert_eq!(command.program, "/bin/roundhoused");
    let state_dir = text(dir.path().join("instances/i"));
    let (workspaces, clones) = (text(dir.path().join("workspaces")), text(dir.path().join("clones")));
    let transport = if cfg!(unix) { "--attach" } else { "--stdio" };
    let given = ["--workspaces-root", &workspaces, "--state-dir", &state_dir, "--clones-dir", &clones];
    assert_eq!(command.args, [&["worker", transport][..], &given].concat());
    let path = command.env.iter().find(|(name, _)| name == "PATH").map(|(_, value)| value.as_str());
    assert!(path.is_some_and(|path| path.split(':').any(|entry| entry == "/opt/tools")), "{path:?}");
}

#[tokio::test]
async fn list_says_which_instances_have_a_worker_running() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    for id in ["a", "b"] {
        provider.create(&id.into(), None, None, id).await.unwrap();
    }
    let mut worker = orphan(dir.path());
    record_worker(&dir.path().join("instances/a"), &worker);

    let mut instances = provider.list().await.unwrap();

    instances.sort_by(|x, y| x.instance_id.cmp(&y.instance_id));
    let states: Vec<_> = instances.iter().map(|i| (i.instance_id.as_str(), i.state)).collect();
    assert_eq!(states, [("a", InstanceState::Running), ("b", InstanceState::Stopped)]);
    worker.kill().unwrap();
    worker.wait().unwrap();
}

#[tokio::test]
async fn stopping_ends_a_worker_still_running_in_the_instance() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();
    provider.create(&id, None, None, "a").await.unwrap();
    let mut worker = orphan(dir.path());
    record_worker(&dir.path().join("instances/a"), &worker);
    let reaper = std::thread::spawn(move || worker.wait().unwrap());

    provider.stop(&id).await.unwrap();

    assert!(reaper.join().unwrap().signal().is_some(), "it should have been signalled");
}

#[tokio::test]
async fn starting_leaves_a_running_instance_alone() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();
    provider.create(&id, None, None, "a").await.unwrap();
    let instance = dir.path().join("instances/a");
    let mut worker = orphan(dir.path());
    record_worker(&instance, &worker);
    let mut agent = orphan(dir.path());
    let agent_record = record_agent(&instance, "s.pid", agent.id(), &process::start_time(agent.id()).unwrap());

    provider.start(&id, &[]).await.unwrap();

    assert!(worker.try_wait().unwrap().is_none(), "the worker serving it was ended");
    assert!(agent.try_wait().unwrap().is_none(), "the worker's agent was ended");
    assert!(agent_record.exists());
    for mut child in [worker, agent] {
        child.kill().unwrap();
        child.wait().unwrap();
    }
}

#[tokio::test]
async fn starting_clears_what_a_killed_worker_left() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();
    provider.create(&id, None, None, "a").await.unwrap();
    let instance = dir.path().join("instances/a");
    let mut agent = orphan(dir.path());
    let agent_record = record_agent(&instance, "s.pid", agent.id(), &process::start_time(agent.id()).unwrap());
    // Reaped as it goes, as in `a_recorded_orphan_is_stopped`, since a child of this test that nobody reaps is still
    // listed, and the start would wait out the agent's grace for it.
    let reaper = std::thread::spawn(move || agent.wait().unwrap());

    provider.start(&id, &[]).await.unwrap();

    assert!(reaper.join().unwrap().signal().is_some(), "the agent it left behind should have been stopped");
    assert!(!agent_record.exists());
}

#[tokio::test]
async fn killing_skips_sigterm_and_returns_once_the_worker_has_gone() {
    use std::os::unix::process::CommandExt;

    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();
    provider.create(&id, None, None, "a").await.unwrap();
    let termed = dir.path().join("termed");
    // Says so if it gets SIGTERM, and ignores it.
    let script = format!("trap ': > {}' TERM; while :; do sleep 0.05; done", termed.display());
    let mut worker = std::process::Command::new("sh").args(["-c", &script]).process_group(0).spawn().unwrap();
    record_worker(&dir.path().join("instances/a"), &worker);
    let (reaped_tx, reaped) = std::sync::mpsc::channel();
    std::thread::spawn(move || reaped_tx.send(worker.wait().unwrap()).unwrap());

    provider.kill(&id).await.unwrap();

    let status = reaped.try_recv().expect("kill returned before the worker had gone");
    assert_eq!(status.signal(), Some(9));
    assert!(!termed.exists(), "a kill shouldn't ask first");
}

#[tokio::test]
async fn base_images_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();

    let made = provider.make_base_image(&id).await;
    let copied = provider.create(&id, Some(&"base".into()), None, "a").await;

    assert!(matches!(made, Err(ProviderError::Unsupported(_))), "{made:?}");
    assert!(matches!(copied, Err(ProviderError::Unsupported(_))), "{copied:?}");
    assert!(!provider.capabilities().has(Feature::BaseImages));
}
