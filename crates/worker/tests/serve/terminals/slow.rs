//! A worker whose hub is slow to read what it sends, as one on a slow link is.

use std::time::Duration;

use protocol::worker::{TerminalShow, WorkerOutcome, WorkerRequest, WorkerResponse};

use super::hub::{Hub, input};

/// About 21 MB of lines, each its number, far more than the worker holds, written as `cat` writes a large file, in
/// large pieces, which a read takes as many of as it can. The command has no number in it as large as the lines', so
/// its echo isn't taken for one.
const WRITES_LINES: &str = "awk 'BEGIN{for(i=1;i<=3*1000*1000;i++)print i}' | cat";

/// The largest number in `bytes`, which for a screen of [`WRITES_LINES`]'s output, whose sequences have only small
/// numbers, is the last line it took in.
fn largest_number(bytes: &[u8]) -> u64 {
    let runs = bytes.split(|byte| !byte.is_ascii_digit()).filter(|run| !run.is_empty());
    runs.filter_map(|run| std::str::from_utf8(run).ok()?.parse().ok()).max().unwrap_or(0)
}

/// The program waits in its PTY once a little of its output waits for the hub, rather than the 1,024 frames the
/// worker's connection holds, and what's sent the hub after it, such as a reply, waits behind little of it. What it
/// takes in has a floor too, so a program that never ran doesn't pass.
#[tokio::test]
async fn a_hub_slow_to_read_holds_the_program_back_and_a_reply_waits_behind_little() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", "echo re''ady").await;
    hub.until_output("ready").await;
    hub.read(false);
    hub.ask(input("t", &format!("{WRITES_LINES}\r"))).await;
    // Nothing says the program has been held back, since what would is waiting for the hub to read it.
    waiting::task::for_what_mustnt_happen(Duration::from_secs(2)).await;
    let asked = hub.ask(WorkerRequest::TerminalShow(TerminalShow { terminal: "t".into() })).await;
    let before = hub.output.len();

    hub.read(true);

    let shown = hub.reply(asked).await;
    let came_first = hub.output.len() - before;
    let WorkerOutcome::Response(WorkerResponse::TerminalShown(shown)) = shown else { panic!("{shown:?}") };
    let taken_in = largest_number(shown.screen.bytes.as_slice());
    assert!(came_first < 256 * 1024, "{came_first} bytes of output came before the reply");
    assert!((20_000..200_000).contains(&taken_in), "the worker took in {taken_in} of the lines");
    hub.call(input("t", "\x03")).await;
    hub.finish().await;
}
