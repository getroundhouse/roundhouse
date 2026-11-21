//! Each terminal a worker runs, from starting its shell to its end.

use std::path::Path;
use std::time::Duration;

use super::hub::{Hub, OPENED_AT_MS, SIZE, WAIT, input, process_records_dir, refused};
use protocol::client::ErrorCode;
use protocol::model::GithubToken;
use protocol::terminal::{Numbering, RowNumber, TerminalExit, TerminalRowsReply, TerminalSize, TerminalUpdate};
use protocol::worker::{
    GitCredential, GitCredentialSet, GithubCredential, RunningTerminalsReply, TerminalFit, TerminalRows, TerminalStop,
    WorkerOutcome, WorkerRequest, WorkerResponse,
};

/// A new size for `t`, keeping no cells.
fn fit(size: TerminalSize) -> WorkerRequest {
    WorkerRequest::TerminalFit(TerminalFit { terminal: "t".into(), size, numbering: Numbering::new(0), keep: vec![] })
}

/// As every process a worker starts is, so `git` and `gh` typed in a terminal use the default GitHub account, as its
/// agents do, rather than the machine's own login, and a token the shell would have had doesn't reach them.
#[tokio::test]
async fn a_terminal_is_given_the_git_credential_its_hub_set() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    let ada = GitCredential { username: "ada".into(), token: GithubToken::new("made-up"), identity: None };
    let set = GitCredentialSet { credential: GithubCredential::Account(ada) };
    assert_eq!(hub.call(WorkerRequest::GitCredentialSet(set)).await, WorkerOutcome::Response(WorkerResponse::Ok));
    hub.open("t", dir.path()).await;

    hub.type_in("t", r#"echo "given ${GIT_CONFIG_COUNT:-none} ${GH_CONFIG_DIR:+dir} ${GH_TOKEN:-none}""#).await;

    hub.until_shown("given 1 dir none").await;
    hub.finish().await;
}

/// What's typed comes back worked out, in the worktree, with the terminal the worker says it is and the environment the
/// hub gave it, but for a parent Claude session's and terminal apps', and starting the terminal again starts nothing
/// more. The worker lists it with the time the hub opened it at.
#[tokio::test]
async fn a_terminal_runs_a_shell_in_the_worktree_that_does_what_is_typed() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;

    hub.open("t", dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in(
        "t",
        r#"echo "$((6*7)) $TERM $PORT ${CLAUDECODE-none} ${TMUX-none} ${GHOSTTY_RESOURCES_DIR-none} $(pwd -P)""#,
    )
    .await;

    let worktree = dir.path().canonicalize().unwrap();
    hub.until_shown(&format!("42 xterm-256color 55000 none none none {}", worktree.display())).await;
    assert_eq!(hub.listed().await, [("t".into(), OPENED_AT_MS)]);
    hub.finish().await;
}

/// A hub asks for rows of a terminal's history by number, in the numbering the worker says they're in, and is sent
/// what its program wrote, oldest first. An ask in another numbering is sent none, and told which it is.
#[tokio::test]
async fn rows_of_history_are_sent_by_number() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", "stty -echo; seq -f 'li''ne %g' 1 60; printf 'do''ne'; sleep 60").await;
    hub.until_shown("done").await;
    let rows = |numbering| {
        let ask = TerminalRows { terminal: "t".into(), numbering, from: RowNumber::new(0), count: 200 };
        WorkerRequest::TerminalRows(ask)
    };

    let WorkerOutcome::Response(WorkerResponse::TerminalRows(told)) = hub.call(rows(Numbering::new(u64::MAX))).await
    else {
        panic!("the worker didn't answer an ask for rows");
    };
    let WorkerOutcome::Response(WorkerResponse::TerminalRows(sent)) = hub.call(rows(told.numbering)).await else {
        panic!("the worker didn't answer an ask for rows");
    };

    assert!(told.rows.is_empty(), "{told:?}");
    let TerminalRowsReply { numbering, first, top, from, rows } = sent;
    assert_eq!((numbering, from), (told.numbering, first));
    assert_eq!(u64::try_from(rows.len()).unwrap(), top.get() - first.get());
    // From the last `line `, since what the shell echoed of the command can share the first line's row. The command
    // splits the word, so its echo isn't taken for a line.
    let lines: Vec<String> = rows
        .iter()
        .map(|row| row.spans.iter().map(|span| span.text.as_str()).collect::<String>())
        .filter_map(|text| text.rfind("line ").map(|at| text[at..].to_owned()))
        .collect();
    let written: Vec<String> = (1..=lines.len()).map(|n| format!("line {n}")).collect();
    assert_eq!(lines, written);
    assert!(lines.len() > 30, "{lines:?}");
    hub.finish().await;
}

/// The program is told of the new size, and so is the hub, in a picture at it. The worker's copy takes it too, and
/// answers where the cursor is from it: as far down and right as it goes, at the new size.
#[tokio::test]
async fn a_new_size_reaches_the_program_the_workers_copy_and_the_hub() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    let size = TerminalSize { cols: 120, rows: 40 };

    let resized = hub.call(fit(size)).await;
    hub.until_pictures(|pictures| pictures.iter().any(|(picture, _)| picture.size == size && picture.whole)).await;
    hub.type_in("t", "stty size").await;
    // The answer is `ESC[40;120R`, read as it comes, with the escape made printable.
    hub.type_in(
        "t",
        r"stty -echo -icanon; printf '\033[999;999H\033[6n'; dd bs=1 count=9 2>/dev/null | tr '\033' E; stty sane",
    )
    .await;

    assert!(matches!(resized, WorkerOutcome::Response(WorkerResponse::TerminalFit(_))), "{resized:?}");
    hub.until_shown("40 120").await;
    hub.until_shown("E[40;120R").await;
    hub.finish().await;
}

/// More than any screen shows would only cost the worker memory. Neither starts or resizes anything.
#[tokio::test]
async fn a_size_with_no_cells_or_too_many_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    let none = TerminalSize { cols: 0, rows: 24 };
    let too_many = TerminalSize { cols: TerminalSize::MOST.cols + 1, rows: 24 };

    let started = hub.start_terminal("t", dir.path(), none).await;
    let listed = hub.listed().await;
    hub.open("t", dir.path()).await;
    let resized = hub.call(fit(too_many)).await;
    hub.type_in("t", "stty size").await;

    assert!(refused(&started, ErrorCode::InvalidRequest), "{started:?}");
    assert_eq!(listed, []);
    assert!(refused(&resized, ErrorCode::InvalidRequest), "{resized:?}");
    hub.until_shown(&format!("{} {}", SIZE.rows, SIZE.cols)).await;
    hub.finish().await;
}

/// Its ID names the files its processes are recorded in, so one that would name a file elsewhere, or that a disk that
/// ignores case would take for another's, starts nothing.
#[tokio::test]
async fn a_terminal_whose_id_cant_name_a_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;

    let started = [hub.start_terminal("../t", dir.path(), SIZE).await, hub.start_terminal("T", dir.path(), SIZE).await];

    for started in &started {
        assert!(refused(started, ErrorCode::InvalidRequest), "{started:?}");
    }
    assert_eq!(hub.listed().await, []);
    hub.finish().await;
}

/// One that can't start, here since its folder has gone, says so, leaves nothing running, and the worker goes on to
/// start the next.
#[tokio::test]
async fn a_terminal_that_cant_start_is_refused_and_the_next_still_starts() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;

    let started = hub.start_terminal("t", &dir.path().join("gone"), SIZE).await;
    let listed = hub.listed().await;
    hub.open("u", dir.path()).await;
    hub.type_in("u", "echo $((6*7))").await;

    assert!(refused(&started, ErrorCode::NotFound), "{started:?}");
    assert_eq!(listed, []);
    hub.until_shown("42").await;
    hub.finish().await;
}

/// A shell that exits ends its terminal with its code, and the worker lists it no longer.
#[tokio::test]
async fn a_shell_that_exits_ends_its_terminal_with_its_code() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;

    hub.type_in("t", "exit 3").await;

    hub.until_update(&TerminalUpdate::Exited(TerminalExit { code: Some(3) })).await;
    let running = hub.call(WorkerRequest::RunningTerminals).await;
    let none = RunningTerminalsReply { terminals: vec![] };
    assert_eq!(running, WorkerOutcome::Response(WorkerResponse::RunningTerminals(none)));
    hub.finish().await;
}

/// So a hub can stop one again, and a hub asking anything of one that has ended hears it's gone.
#[tokio::test]
async fn stopping_a_terminal_again_changes_nothing_and_it_shows_nothing_after() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    let stop = || WorkerRequest::TerminalStop(TerminalStop { terminal: "t".into() });

    let stopped = hub.call(stop()).await;
    let again = hub.call(stop()).await;
    let rows = TerminalRows { terminal: "t".into(), numbering: Numbering::new(0), from: RowNumber::new(0), count: 1 };
    let shown = hub.call(WorkerRequest::TerminalRows(rows)).await;

    assert_eq!(
        (stopped, again),
        (WorkerOutcome::Response(WorkerResponse::Ok), WorkerOutcome::Response(WorkerResponse::Ok))
    );
    assert!(refused(&shown, ErrorCode::NotFound), "{shown:?}");
    hub.finish().await;
}

/// A worker's timers, but for a stop grace short enough that a test of what doesn't end when it's hung up on doesn't wait
/// out a worker's.
const SHORT_STOP_GRACE: worker::Timers =
    worker::Timers { terminal_stop_grace: Duration::from_millis(200), ..worker::Timers::FOR_TESTS };

/// Closing a terminal's tab ends its shell, and the job it runs, even one that doesn't end when it's hung up on, and
/// even while the shell runs something else in front as it exits, as its exit trap's `sleep` is here.
#[tokio::test]
async fn stopping_a_terminal_ends_its_shell_and_the_job_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start_with(dir.path(), SHORT_STOP_GRACE).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", r#"trap "sleep 60" EXIT; echo "shell $$""#).await;
    let shell = hub.until_pid("shell ").await;
    hub.type_in("t", r#"sh -c 'trap "" HUP; echo "job $$"; exec sleep 60'"#).await;
    let job = hub.until_pid("job ").await;

    let stopped = hub.call(WorkerRequest::TerminalStop(TerminalStop { terminal: "t".into() })).await;

    assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
    assert!(matches!(hub.updates.last(), Some(TerminalUpdate::Exited(_))), "{:?}", hub.updates);
    gone(shell).await;
    gone(job).await;
    hub.finish().await;
}

/// So the provider can stop them if the worker is killed: the shell as it starts, and the job it runs in front once the
/// worker has looked, as often as its timers say, until that job ends. Nothing is left recorded once the terminal has.
#[tokio::test]
async fn a_terminal_records_its_shell_and_the_job_in_front_while_they_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", r#"echo "shell $$""#).await;
    let shell = hub.until_pid("shell ").await;
    until_recorded(dir.path(), "terminal-t.pid", Some(shell)).await;

    hub.type_in("t", r#"sh -c 'echo "job $$"; exec sleep 60'"#).await;
    let job = hub.until_pid("job ").await;
    until_recorded(dir.path(), "terminal-t.front.pid", Some(job)).await;
    // Control-C, which ends the job, so the shell is in front again.
    hub.call(input("t", "\x03")).await;
    until_recorded(dir.path(), "terminal-t.front.pid", None).await;
    hub.finish().await;

    let left: Vec<_> =
        std::fs::read_dir(process_records_dir(dir.path())).unwrap().flatten().map(|entry| entry.file_name()).collect();
    assert!(left.is_empty(), "{left:?}");
}

/// A stop gives the job in front a grace to end, and a worker killed in it leaves the job recorded, so the provider
/// still stops it: it's recorded until it has ended, or been killed. The grace is shorter here than a worker's, and long
/// enough to read the record in.
#[tokio::test]
async fn a_terminal_being_stopped_keeps_its_job_recorded_until_the_job_has_ended() {
    let dir = tempfile::tempdir().unwrap();
    let timers = worker::Timers { terminal_stop_grace: Duration::from_millis(500), ..worker::Timers::FOR_TESTS };
    let mut hub = Hub::start_with(dir.path(), timers).await;
    hub.open("t", dir.path()).await;
    let hupped = dir.path().join("hupped");
    // It notes the hangup and runs on, until it's killed. It waits on its `sleep` with `wait`, which a hangup interrupts,
    // since `sh` holds a trap until the command it runs in front ends. A hangup that comes before `sleep` has started
    // doesn't end that `sleep`, which would hold the note for a minute.
    let job =
        format!(r#"sh -c 'trap ": > {}" HUP; echo "job $$"; while :; do sleep 60 & wait $!; done'"#, hupped.display());
    hub.type_in("t", &job).await;
    let job = hub.until_pid("job ").await;
    until_recorded(dir.path(), "terminal-t.front.pid", Some(job)).await;
    let front = process_records_dir(dir.path()).join("terminal-t.front.pid");
    let while_stopping = async {
        waiting::task::until(WAIT, "the job was never hung up on", || hupped.exists()).await;
        process::ProcessRecord::read(&front).map(|record| record.pid)
    };

    let stop = hub.call(WorkerRequest::TerminalStop(TerminalStop { terminal: "t".into() }));
    let (stopped, recorded) = tokio::join!(stop, while_stopping);

    assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
    assert_eq!(recorded, Some(job), "it was recorded as it was hung up on");
    assert_eq!(process::ProcessRecord::read(&front), None);
    gone(job).await;
    hub.finish().await;
}

/// It's killed once it has had as long as it gets to end, so a stop always ends.
#[tokio::test]
async fn stopping_a_terminal_ends_a_shell_that_doesnt_end_when_its_hung_up_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start_with(dir.path(), SHORT_STOP_GRACE).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", r#"trap "" HUP; echo "shell $$""#).await;
    let shell = hub.until_pid("shell ").await;

    let stopped = hub.call(WorkerRequest::TerminalStop(TerminalStop { terminal: "t".into() })).await;

    assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
    assert!(matches!(hub.updates.last(), Some(TerminalUpdate::Exited(_))), "{:?}", hub.updates);
    gone(shell).await;
    hub.finish().await;
}

/// As archiving a workspace stops its worker, which sends the hub each terminal's end before it goes.
#[tokio::test]
async fn a_worker_that_stops_ends_its_terminals() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", r#"echo "shell $$""#).await;
    let shell = hub.until_pid("shell ").await;

    let updates = hub.finish().await;

    assert!(matches!(updates.last(), Some(TerminalUpdate::Exited(_))), "{updates:?}");
    gone(shell).await;
}

#[tokio::test]
async fn typing_into_a_terminal_that_isnt_running_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;

    let typed = hub.call(input("t", "ls")).await;

    assert!(refused(&typed, ErrorCode::NotFound), "{typed:?}");
    hub.finish().await;
}

/// Until the worker has recorded `pid` as `name`, or nothing, as it does once it has looked at what's in front.
async fn until_recorded(dir: &Path, name: &str, pid: Option<u32>) {
    let path = process_records_dir(dir).join(name);
    let recorded = || process::ProcessRecord::read(&path).map(|record| record.pid);
    waiting::task::until_ok(WAIT, || match recorded() {
        found if found == pid => Ok(()),
        found => Err(format!("{name} records {found:?}, not {pid:?}")),
    })
    .await;
}

/// Once there's no process with `pid`.
async fn gone(pid: u32) {
    if !waiting::task::up_to(WAIT, || process::start_time(pid).is_none()).await {
        let mut ps = std::process::Command::new("ps");
        let ps = ps.args(["-o", "pid,ppid,pgid,stat,command", "-p"]).arg(pid.to_string()).output().unwrap();
        panic!("process {pid} is still running: {}", String::from_utf8_lossy(&ps.stdout))
    }
}
