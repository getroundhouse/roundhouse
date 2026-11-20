//! A worker's terminal that no client is attached to, which it sends only its shell's end of.

use protocol::worker::{TerminalShow, WorkerRequest};

use super::super::hello;
use super::hub::{Hub, WAIT, contains, input};

/// About 2 MB of lines, six times what a worker held back by a hub that isn't reading takes in, written as `cat`
/// writes a large file, and then a file to say it's done.
const WRITES_LINES_THEN_DONE: &str = "awk 'BEGIN{for(i=1;i<=300*1000;i++)print i}' | cat; touch done";

/// Neither held back nor sent: the program finishes, though the hub has stopped reading, where one held back would
/// wait for it, and nothing it writes reaches the hub.
#[tokio::test]
async fn a_terminal_no_client_is_attached_to_sends_nothing_and_isnt_held_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.type_in("t", "echo re''ady").await;
    hub.until_output("ready").await;
    hub.attached("t", false).await;
    hub.read(false);
    hub.ask(input("t", &format!("{WRITES_LINES_THEN_DONE}\r"))).await;
    let done = dir.path().join("done");
    waiting::task::until(WAIT, "the program was held back", || done.exists()).await;
    let asked = hub.ask(WorkerRequest::TerminalShow(TerminalShow { terminal: "t".into() })).await;
    let before = hub.output.len();

    hub.read(true);

    hub.reply(asked).await;
    // Whatever was on its way as it was told, such as the shell's prompt.
    let came = hub.output.len() - before;
    assert!(came < 1024, "{came} bytes of output were sent");
    hub.finish().await;
}

/// As a hub that connects takes them to be, which one from before it could say otherwise doesn't. What the program
/// wrote meanwhile is in what the terminal shows, and the updates after go on with no gap, which the test hub checks.
#[tokio::test]
async fn a_hello_has_every_terminal_send_what_its_program_writes_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    hub.attached("t", false).await;
    hub.type_in("t", "echo un''seen; touch ran").await;
    let ran = dir.path().join("ran");
    waiting::task::until(WAIT, "the program never ran", || ran.exists()).await;
    let shown = hub.shown("t").await;
    assert!(contains(shown.screen.bytes.as_slice(), "unseen"), "what it wrote is in what the terminal shows");
    assert!(!contains(&hub.output, "unseen"), "sent while no client was attached");

    hub.call(hello()).await;
    hub.type_in("t", "echo se''en").await;

    hub.until_output("seen").await;
    hub.finish().await;
}
