use super::*;
use std::os::unix::process::ExitStatusExt;

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

/// Each is asked at once, so they share one grace, rather than each waiting for the one before. Each here stops only
/// once both have been asked, so one asked only after the other had gone would wait out a grace far longer than the
/// test waits.
#[tokio::test]
async fn recorded_groups_are_all_asked_to_stop_before_any_is_waited_for() {
    const PATIENT: Duration = Duration::from_secs(600);
    let dir = tempfile::tempdir().unwrap();
    let at = |name: &str| dir.path().join(name).display().to_string();
    let until_both_asked = |me: &str, other: &str| {
        let (asked, other_asked, ready) = (at(&format!("{me}.asked")), at(&format!("{other}.asked")), at(me));
        format!(
            "trap ': > {asked}' TERM; : > {ready}; until [ -f {asked} ] && [ -f {other_asked} ]; do sleep 0.05; done"
        )
    };
    let (first, first_reaped) = recorded_group(dir.path(), "a.pid", &until_both_asked("a", "b"));
    let (second, second_reaped) = recorded_group(dir.path(), "b.pid", &until_both_asked("b", "a"));
    // A shell asked before it gets to its `trap` would stop when asked, rather than once both had been.
    for ready in ["a", "b"] {
        until_there(Path::new(&at(ready)), "a group never got to waiting to be asked").await;
    }

    let instance = dir.path().to_owned();
    let stopping = tokio::spawn(async move { stop_orphans(&instance, PATIENT).await });
    let stopped = waiting::task::up_to(Duration::from_secs(10), || stopping.is_finished()).await;
    let left = [first, second].map(killed_if_left);

    assert!(stopped, "one was asked only once the other had gone, or neither was asked");
    assert_eq!(left, [false, false], "each stopped");
    for reaped in [first_reaped, second_reaped] {
        assert_eq!(reaped.join().unwrap().signal(), None, "each stopped by itself, once both had been asked");
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
    provider_dialing(dir, "/bin/roundhoused".into(), false)
}

/// One whose worker is `worker_exe`, and whose workers dial the hub if `dials`.
fn provider_dialing(dir: &Path, worker_exe: PathBuf, dials: bool) -> LocalProvider {
    LocalProvider::new(LocalConfig {
        worker_exe,
        instances_dir: dir.join("instances"),
        workspaces_root: dir.join("workspaces"),
        clones_dir: dir.join("clones"),
        worker_logs_dir: dir.join("worker-logs"),
        environment: process::Environment::NONE.with("GIVEN", "the machine's"),
        dials,
        hub_address_file: Some(dir.join("loopback-worker-address.json")),
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
        provider.create(&id, None, None).await.unwrap();
        provider.start(&id, &[], None).await.unwrap();
    }
    let first = provider.connect(&id).await.unwrap();
    let second = provider.connect(&id).await.unwrap();
    for _ in 0..2 {
        provider.end_worker(&id).await.unwrap();
        provider.kill_worker(&id).await.unwrap();
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
    provider.create(&id, None, None).await.unwrap();

    let command = provider.connect(&id).await.unwrap();

    let text = |path: PathBuf| path.to_string_lossy().into_owned();
    assert_eq!(command.program, "/bin/roundhoused");
    let state_dir = text(dir.path().join("instances/i"));
    let (workspaces, clones) = (text(dir.path().join("workspaces")), text(dir.path().join("clones")));
    let given = ["--workspaces-root", &workspaces, "--state-dir", &state_dir, "--clones-dir", &clones];
    assert_eq!(command.args, [&["worker", "--stdio"][..], &given].concat());
    assert_eq!(command.env, [], "the hub gives it the machine's environment");
}

/// Where its workers dial, the only one the hub reaches by a command is one an earlier build left listening.
#[tokio::test]
async fn where_workers_dial_connect_reaches_one_an_earlier_build_left_listening() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider_dialing(dir.path(), "/bin/roundhoused".into(), true);
    let id: InstanceId = "i".into();
    provider.create(&id, None, None).await.unwrap();

    let command = provider.connect(&id).await.unwrap();

    assert_eq!(provider.workers_reach(), crate::WorkerReach::Loopback);
    assert_eq!(command.args[..2], ["worker", "--attach"]);
}

/// Any user on the machine can read a process's arguments, so the ticket goes on the worker's stdin, where only the
/// worker reads it, and the worker is given the machine's environment and its log.
#[tokio::test]
async fn a_worker_that_dials_is_given_its_ticket_on_stdin_and_not_in_its_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let said = dir.path().join("said");
    let exe = dir.path().join("roundhoused");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}.args'\ncat > '{0}.stdin'\nprintf '%s' \"$GIVEN\" > '{0}.env'\n",
        said.display()
    );
    process::testing::write_program(&exe, script).unwrap();
    let provider = provider_dialing(dir.path(), exe, true);
    let id: InstanceId = "i".into();
    provider.create(&id, None, None).await.unwrap();
    let ticket = protocol::dialing::WorkerTicket::from("the-secret-ticket".to_string());

    provider.start(&id, &[], Some(&ticket)).await.unwrap();

    let args = std::fs::read_to_string(said.with_extension("args")).unwrap();
    let args: Vec<&str> = args.lines().collect();
    assert!(!args.iter().any(|arg| arg.contains("the-secret-ticket")), "{args:?}");
    let after = |flag: &str| args.iter().position(|arg| *arg == flag).map(|at| args[at + 1]);
    assert_eq!(after("--ticket"), Some("-"), "{args:?}");
    let address_file = dir.path().join("loopback-worker-address.json");
    assert_eq!(after("--hub-address-file"), Some(address_file.to_str().unwrap()), "so it finds a hub that moved");
    assert!(args.contains(&"--detach"), "so it outlives the hub: {args:?}");
    let log = dir.path().join("worker-logs/i.log");
    assert_eq!(after("--log"), Some(log.to_str().unwrap()));
    assert_eq!(std::fs::read_to_string(said.with_extension("stdin")).unwrap(), "the-secret-ticket");
    assert_eq!(std::fs::read_to_string(said.with_extension("env")).unwrap(), "the machine's");
}

#[tokio::test]
async fn a_worker_that_dials_isnt_started_without_a_ticket() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider_dialing(dir.path(), "/bin/roundhoused".into(), true);
    let id: InstanceId = "i".into();
    provider.create(&id, None, None).await.unwrap();

    let started = provider.start(&id, &[], None).await;

    assert!(matches!(started, Err(ProviderError::Unsupported(_))), "{started:?}");
}

/// A directory is never asleep, so every instance is running, and a worker is on the one it's recorded in.
#[tokio::test]
async fn it_says_which_instances_have_a_worker_running() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    for id in ["a", "b"] {
        provider.create(&id.into(), None, None).await.unwrap();
    }
    let mut worker = orphan(dir.path());
    record_worker(&dir.path().join("instances/a"), &worker);

    let mut instances = provider.list().await.unwrap();
    let running = provider.workers_running().await.unwrap();

    instances.sort_by(|x, y| x.instance_id.cmp(&y.instance_id));
    let states: Vec<_> = instances.iter().map(|i| (i.instance_id.as_str(), i.state)).collect();
    assert_eq!(states, [("a", InstanceState::Running), ("b", InstanceState::Running)]);
    assert_eq!(running, [InstanceId::from("a")]);
    worker.kill().unwrap();
    worker.wait().unwrap();
}

#[tokio::test]
async fn ending_the_worker_ends_one_still_running_in_the_instance() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();
    provider.create(&id, None, None).await.unwrap();
    let mut worker = orphan(dir.path());
    record_worker(&dir.path().join("instances/a"), &worker);
    let reaper = std::thread::spawn(move || worker.wait().unwrap());

    provider.end_worker(&id).await.unwrap();

    assert!(reaper.join().unwrap().signal().is_some(), "it should have been signalled");
}

#[tokio::test]
async fn starting_leaves_a_running_instance_alone() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();
    provider.create(&id, None, None).await.unwrap();
    let instance = dir.path().join("instances/a");
    let mut worker = orphan(dir.path());
    record_worker(&instance, &worker);
    let mut agent = orphan(dir.path());
    let agent_record = record_agent(&instance, "s.pid", agent.id(), &process::start_time(agent.id()).unwrap());

    provider.start(&id, &[], None).await.unwrap();

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
    provider.create(&id, None, None).await.unwrap();
    let instance = dir.path().join("instances/a");
    let mut agent = orphan(dir.path());
    let agent_record = record_agent(&instance, "s.pid", agent.id(), &process::start_time(agent.id()).unwrap());
    // Reaped as it goes, as in `a_recorded_orphan_is_stopped`, since a child of this test that nobody reaps is still
    // listed, and the start would wait out the agent's grace for it.
    let reaper = std::thread::spawn(move || agent.wait().unwrap());

    provider.start(&id, &[], None).await.unwrap();

    assert!(reaper.join().unwrap().signal().is_some(), "the agent it left behind should have been stopped");
    assert!(!agent_record.exists());
}

#[tokio::test]
async fn killing_the_worker_skips_sigterm_and_returns_once_it_has_gone() {
    use std::os::unix::process::CommandExt;

    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();
    provider.create(&id, None, None).await.unwrap();
    let termed = dir.path().join("termed");
    // Says so if it gets SIGTERM, and ignores it.
    let script = format!("trap ': > {}' TERM; while :; do sleep 0.05; done", termed.display());
    let mut worker = std::process::Command::new("sh").args(["-c", &script]).process_group(0).spawn().unwrap();
    record_worker(&dir.path().join("instances/a"), &worker);
    let (reaped_tx, reaped) = std::sync::mpsc::channel();
    std::thread::spawn(move || reaped_tx.send(worker.wait().unwrap()).unwrap());

    provider.kill_worker(&id).await.unwrap();

    let status = reaped.try_recv().expect("kill_worker returned before the worker had gone");
    assert_eq!(status.signal(), Some(9));
    assert!(!termed.exists(), "a kill shouldn't ask first");
}

#[tokio::test]
async fn base_images_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let provider = provider(dir.path());
    let id: InstanceId = "a".into();

    let made = provider.make_base_image(&id).await;
    let copied = provider.create(&id, Some(&"base".into()), None).await;

    assert!(matches!(made, Err(ProviderError::Unsupported(_))), "{made:?}");
    assert!(matches!(copied, Err(ProviderError::Unsupported(_))), "{copied:?}");
    assert!(!provider.capabilities().has(Feature::BaseImages));
}
