//! How the commands that work until a file a test makes exists, `slow-until`, `think-until`, `hang-until`,
//! `request-until`, `agent-until` and `agent-foreground-until`, wait on it, and how the tool `hang` runs until the turn
//! is interrupted. While one waits on a file, a prompt that reaches the agent makes `FILE.prompted` once it's queued to
//! be taken into the turn, so a test can wait for that before making FILE. A prompt the hub has recorded may not have
//! reached the agent yet, since the hub sends it on after.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use parking_lot::Mutex;

use crate::turn::Turn;
use crate::{Input, Prompt, pause};

/// The file a command waits on, while it does.
static WAITING_ON: Mutex<Option<PathBuf>> = Mutex::new(None);

/// How often [`pause_until`] looks again.
const POLL: Duration = Duration::from_millis(20);

/// The file a command waits on, which says the command waits on it for as long as it's held.
pub(crate) struct WaitingOn(PathBuf);

impl WaitingOn {
    pub(crate) fn file(file: &Path) -> Self {
        *WAITING_ON.lock() = Some(file.to_path_buf());
        Self(file.to_path_buf())
    }

    /// Whether the test has made the file.
    pub(crate) fn made(&self) -> bool {
        self.0.exists()
    }
}

impl Drop for WaitingOn {
    fn drop(&mut self) {
        *WAITING_ON.lock() = None;
    }
}

/// Says a prompt has reached the agent, if a command waits on a file. Only once the prompt is queued, so the turn takes
/// it in however soon after the command ends.
pub(crate) fn a_prompt_is_queued() {
    if let Some(file) = WAITING_ON.lock().as_deref() {
        std::fs::write(format!("{}.prompted", file.display()), "").unwrap();
    }
}

/// Numbered chunks of the text `FILE:TEXT` gives, until FILE exists. Each is made only once the one before has gone, so
/// what they're said in ends as soon as the file exists, and they wait on it until then. Each that has gone adds a line
/// to `FILE.said`, so a test can wait for the agent to have said more. The text is what follows the last colon, since a
/// path on Windows has one after its drive.
pub(crate) fn until(file_and_text: &str) -> impl Iterator<Item = String> + use<> {
    let (file, text) = file_and_text.rsplit_once(':').unwrap();
    let said = PathBuf::from(format!("{file}.said"));
    let (waiting, text) = (WaitingOn::file(Path::new(file)), text.to_string());
    (0..).map_while(move |i| {
        if i > 0 {
            let mut said = std::fs::OpenOptions::new().create(true).append(true).open(&said).unwrap();
            std::io::Write::write_all(&mut said, b"said\n").unwrap();
        }
        (!waiting.made()).then(|| format!("{text}{i} "))
    })
}

/// A tool that runs until `file` exists, or with none, until the turn is interrupted. Returns true if it was.
pub(crate) fn hang(
    turn: &mut Turn,
    file: Option<&Path>,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let tool_use_id = turn.tool_use("hang");
    let interrupted = match file {
        Some(file) => pause_until_made(file, rx, queued, eof),
        None => pause_until(|| false, rx, queued, eof),
    };
    if interrupted {
        turn.tool_result(&tool_use_id, "stopped", true);
        return true;
    }
    turn.tool_result(&tool_use_id, "finished hanging", false);
    false
}

/// Waits until `file` exists, saying nothing, as a tool does while it runs, or a request while its answer comes, and
/// says when a prompt has reached the agent meanwhile. Returns true if the turn was interrupted first.
pub(crate) fn pause_until_made(
    file: &Path,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let waiting = WaitingOn::file(file);
    pause_until(|| waiting.made(), rx, queued, eof)
}

/// Waits until `done` says. Input closing doesn't end it, as it doesn't end the real CLI's tool, which the CLI waits
/// for before it finishes its turn and exits. Returns true if the turn was interrupted first.
fn pause_until(done: impl Fn() -> bool, rx: &Receiver<Input>, queued: &mut VecDeque<Prompt>, eof: &mut bool) -> bool {
    while !done() {
        if pause(rx, POLL, queued, eof) {
            return true;
        }
    }
    false
}
