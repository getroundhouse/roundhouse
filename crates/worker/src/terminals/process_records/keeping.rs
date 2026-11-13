//! A terminal's process records, which the terminal writes and removes as what it runs changes.

use std::io;
use std::path::{Path, PathBuf};

use process::ProcessRecord;
use protocol::model::TerminalId;
use tracing::warn;

/// A terminal's process records, which it writes and removes itself, in the order what it runs changes, since writing
/// one is a small file and the kernel's note of when the process started. Dropped before [`ProcessRecords::finished`],
/// as when the terminal's thread unwinds, they kill what they record, since nothing else will end the shell or the job
/// in front, as a call to an agent's CLI dropped unfinished does. They're the terminal's own, rather than the guard a
/// call to an agent's CLI has, since what they record changes while the terminal runs.
pub(in crate::terminals) struct ProcessRecords {
    shell: PathBuf,
    front: PathBuf,
    /// The group last recorded as the job in front, so it's written again only once another is.
    in_front: Option<u32>,
    /// Whether the terminal has finished with them, so dropping them kills nothing.
    finished: bool,
}

impl ProcessRecords {
    /// The records of `terminal`'s processes, in `dir`. No terminal's ID has a `.`, so neither name can be another
    /// terminal's.
    pub(in crate::terminals) fn new(dir: &Path, terminal: &TerminalId) -> Self {
        let shell = dir.join(format!("terminal-{terminal}.pid"));
        Self { shell, front: dir.join(format!("terminal-{terminal}.front.pid")), in_front: None, finished: false }
    }

    pub(in crate::terminals) fn shell(&self, pid: u32) {
        write(&self.shell, pid);
    }

    /// The group of the job the shell runs in front, or `None` while the shell is in front itself.
    pub(in crate::terminals) fn front(&mut self, front: Option<u32>) {
        if front != self.in_front {
            self.in_front = front;
            match front {
                Some(group) => write(&self.front, group),
                None => remove(&self.front),
            }
        }
    }

    /// Once the terminal has ended: its shell has, and if the terminal was asked to end, so has the job in front, by
    /// itself or killed. A job that outlasts a shell that ended without being asked was left running on purpose, so it
    /// isn't kept recorded. Removes both records, so a worker that stops leaves neither.
    pub(in crate::terminals) fn finished(&mut self) {
        self.finished = true;
        remove(&self.shell);
        remove(&self.front);
    }
}

impl Drop for ProcessRecords {
    /// Unless the terminal had finished with them, kills each group whose record still proves it's that group's, and
    /// removes the records.
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        for path in [&self.shell, &self.front] {
            if let Some(record) = ProcessRecord::read(path).filter(ProcessRecord::is_running) {
                process::signal_group(record.pid, process::Signal::Kill);
            }
            remove(path);
        }
    }
}

fn write(path: &Path, pid: u32) {
    if let Err(e) = ProcessRecord::write(path, pid) {
        warn!("couldn't record {}, so the provider can't stop it if the worker is killed: {e}", path.display());
    }
}

fn remove(path: &Path) {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => warn!("couldn't remove {}: {e}", path.display()),
        _ => {}
    }
}
