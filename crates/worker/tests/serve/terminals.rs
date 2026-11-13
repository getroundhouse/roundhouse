//! A worker's terminals, driven as a hub drives them, each running `sh` in a PTY.

use std::time::Duration;

use hub::{Hub, contains, input, refused};
use protocol::client::ErrorCode;
use protocol::terminal::{TerminalExit, TerminalSize, TerminalUpdate};
use protocol::worker::{
    RunningTerminalsReply, TerminalResize, TerminalShow, TerminalShown, TerminalStop, WorkerOutcome, WorkerRequest,
    WorkerResponse,
};
use tokio::time::timeout;

mod hub;

pub(super) const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
/// When the hub says it opened each terminal.
pub(super) const OPENED_AT_MS: i64 = 5;
/// Long enough for a shell to start on a busy machine.
pub(super) const WAIT: Duration = Duration::from_secs(20);

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
    hub.until_output(&format!("42 xterm-256color 55000 none none none {}", worktree.display())).await;
    assert_eq!(hub.listed().await, [("t".into(), OPENED_AT_MS)]);
    hub.finish().await;
}

/// A hub asks what a terminal shows as a client attaches, and sends the client what comes after.
#[tokio::test]
async fn what_a_terminal_shows_takes_in_everything_its_program_wrote_before() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", "echo $((6*7))").await;
    let seen = hub.until_output("42").await;

    let TerminalShown { screen, next } = hub.shown("t").await;

    assert_eq!(screen.size, SIZE);
    assert!(contains(screen.bytes.as_slice(), "42"), "{:?}", String::from_utf8_lossy(screen.bytes.as_slice()));
    assert!(next >= seen, "the screen takes in {next:?} updates, and {seen:?} had been sent");
    hub.finish().await;
}

/// A screen says the first update it doesn't take in is the next the worker sends, so a hub sends a client neither one
/// it has twice nor less than everything. Here nothing is written after `ready`, so the next update is the one after
/// the last that came, and the same size again sends none.
#[tokio::test]
async fn a_screen_takes_in_every_update_before_it_and_none_after() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    // Split, so the echo of what's typed isn't what's waited for, and `sleep` keeps the shell from writing a prompt.
    hub.type_in("t", "stty -echo; printf 'rea''dy'; sleep 60").await;
    hub.until_output("ready").await;
    let size = TerminalSize { cols: 100, rows: 30 };
    let resize = || WorkerRequest::TerminalResize(TerminalResize { terminal: "t".into(), size });
    hub.call(resize()).await;
    hub.until_update(&TerminalUpdate::Resized { size }).await;

    let first = hub.shown("t").await;
    hub.call(resize()).await;
    let again = hub.shown("t").await;

    assert_eq!(first.next, hub.next[&"t".into()]);
    assert_eq!(first.screen.size, size);
    let shows = String::from_utf8_lossy(first.screen.bytes.as_slice());
    assert!(contains(first.screen.bytes.as_slice(), "ready"), "{shows:?}");
    assert_eq!(again.next, first.next);
    hub.finish().await;
}

/// The program is told of the new size, and so is every client, in order with what the program writes. The worker's
/// copy takes it too, and answers where the cursor is from it: as far down and right as it goes, at the new size.
#[tokio::test]
async fn a_new_size_reaches_the_program_the_workers_copy_and_the_hub() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    let size = TerminalSize { cols: 120, rows: 40 };

    let resized = hub.call(WorkerRequest::TerminalResize(TerminalResize { terminal: "t".into(), size })).await;
    // Every client is told once the PTY has it, and typing goes to the program by another way, which can be quicker.
    hub.until_update(&TerminalUpdate::Resized { size }).await;
    hub.type_in("t", "stty size").await;
    // The answer is `ESC[40;120R`, read as it comes, with the escape made printable.
    hub.type_in(
        "t",
        r"stty -echo -icanon; printf '\033[999;999H\033[6n'; dd bs=1 count=9 2>/dev/null | tr '\033' E; stty sane",
    )
    .await;

    assert_eq!(resized, WorkerOutcome::Response(WorkerResponse::Ok));
    hub.until_output("40 120").await;
    hub.until_output("E[40;120R").await;
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
    let resized =
        hub.call(WorkerRequest::TerminalResize(TerminalResize { terminal: "t".into(), size: too_many })).await;

    assert!(refused(&started, ErrorCode::InvalidRequest), "{started:?}");
    assert_eq!(listed, []);
    assert!(refused(&resized, ErrorCode::InvalidRequest), "{resized:?}");
    assert_eq!(hub.shown("t").await.screen.size, SIZE);
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
    hub.until_output("42").await;
    hub.finish().await;
}

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

/// So a hub can stop one again, and a hub asking what one shows that has ended hears it's gone.
#[tokio::test]
async fn stopping_a_terminal_again_changes_nothing_and_it_shows_nothing_after() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    let stop = || WorkerRequest::TerminalStop(TerminalStop { terminal: "t".into() });

    let stopped = hub.call(stop()).await;
    let again = hub.call(stop()).await;
    let shown = hub.call(WorkerRequest::TerminalShow(TerminalShow { terminal: "t".into() })).await;

    assert_eq!(
        (stopped, again),
        (WorkerOutcome::Response(WorkerResponse::Ok), WorkerOutcome::Response(WorkerResponse::Ok))
    );
    assert!(refused(&shown, ErrorCode::NotFound), "{shown:?}");
    hub.finish().await;
}

/// Closing a terminal's tab ends its shell, and the job it runs, even one that doesn't end when it's hung up on.
#[tokio::test]
async fn stopping_a_terminal_ends_its_shell_and_the_job_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", r#"echo "shell $$""#).await;
    let shell = hub.until_pid("shell ").await;
    hub.type_in("t", r#"sh -c 'trap "" HUP; echo "job $$"; while :; do sleep 1; done'"#).await;
    let job = hub.until_pid("job ").await;

    let stopped = hub.call(WorkerRequest::TerminalStop(TerminalStop { terminal: "t".into() })).await;

    assert_eq!(stopped, WorkerOutcome::Response(WorkerResponse::Ok));
    assert!(matches!(hub.updates.last(), Some(TerminalUpdate::Exited(_))), "{:?}", hub.updates);
    gone(shell).await;
    gone(job).await;
    hub.finish().await;
}

/// It's killed once it has had as long as it gets to end, so a stop always ends.
#[tokio::test]
async fn stopping_a_terminal_ends_a_shell_that_doesnt_end_when_its_hung_up_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
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

/// Once there's no process with `pid`.
async fn gone(pid: u32) {
    timeout(WAIT, async {
        while process::start_time(pid).is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        let mut ps = std::process::Command::new("ps");
        let ps = ps.args(["-o", "pid,ppid,pgid,stat,command", "-p"]).arg(pid.to_string()).output().unwrap();
        panic!("process {pid} is still running: {}", String::from_utf8_lossy(&ps.stdout))
    });
}
