//! How the commands that work until a file a test makes exists, `slow-until`, `think-until`, `hang-until` and
//! `agent-until`, wait on it, and the tool `hang:MS` runs for a time as `hang-until` does until then. While one waits
//! on a file, a prompt that reaches the agent makes `FILE.prompted` once it's queued to be taken into the turn, so a
//! test can wait for that before making FILE. A prompt the hub has recorded may not have reached
//! the agent yet, since the hub sends it on after.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::turn::Turn;
use crate::{Input, Prompt, pause};

/// The file a command waits on, while it does.
static WAITING_ON: Mutex<Option<PathBuf>> = Mutex::new(None);

/// How often [`hang`] looks again.
const HANG_POLL: Duration = Duration::from_millis(50);

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
/// what they're said in ends as soon as the file exists, and they wait on it until then.
pub(crate) fn until(file_and_text: &str) -> impl Iterator<Item = String> + use<> {
    let (file, text) = file_and_text.split_once(':').unwrap();
    let (waiting, text) = (WaitingOn::file(Path::new(file)), text.to_string());
    (0..).map_while(move |i| (!waiting.made()).then(|| format!("{text}{i} ")))
}

/// A tool that runs until `done` says, given how long it has run. Returns true if the turn was interrupted.
pub(crate) fn hang(
    turn: &mut Turn,
    done: impl Fn(Duration) -> bool,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let tool_use_id = turn.tool_use("hang");
    let started = Instant::now();
    while !done(started.elapsed()) {
        if pause(rx, HANG_POLL, queued, eof) {
            turn.tool_result(&tool_use_id, "stopped", true);
            return true;
        }
    }
    turn.tool_result(&tool_use_id, "finished hanging", false);
    false
}
