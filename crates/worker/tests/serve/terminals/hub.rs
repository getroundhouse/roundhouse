//! The hub's end of a worker's stream, for the terminal tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::frame;
use protocol::model::TerminalId;
use protocol::paths::AGENTS_DIR;
use protocol::terminal::{TerminalPicture, TerminalSize, TerminalUpdate, UpdateIndex};
use protocol::worker::{
    RunningTerminalsReply, TerminalInput, TerminalShow, TerminalShown, TerminalStart, WorkerCall, WorkerFrame,
    WorkerOutcome, WorkerRequest, WorkerResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, WriteHalf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};

use super::super::{config, hello, start_worker};

pub(super) const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
/// When the hub says it opened each terminal.
pub(super) const OPENED_AT_MS: i64 = 5;
/// Long enough for a shell to start on a busy machine.
pub(super) const WAIT: Duration = Duration::from_secs(20);

/// The hub's end of a worker's stream, keeping what its terminals send.
pub(super) struct Hub {
    write: WriteHalf<DuplexStream>,
    frames: mpsc::UnboundedReceiver<WorkerFrame>,
    running: JoinHandle<()>,
    calls: u64,
    replies: HashMap<u64, WorkerOutcome>,
    /// Everything the terminals' programs wrote, in order.
    pub(super) output: Vec<u8>,
    /// How much of `output` the waits so far have found what they waited for in.
    searched: usize,
    /// Every update but output, in order.
    pub(super) updates: Vec<TerminalUpdate>,
    /// How many of `updates` the waits so far have found what they waited for in.
    updates_searched: usize,
    /// The `at` each terminal's next update should have.
    pub(super) next: HashMap<TerminalId, UpdateIndex>,
    /// Every frame of a picture, in order, with when it came.
    pub(super) pictures: Vec<(TerminalPicture, Instant)>,
}

impl Hub {
    /// A worker for workspaces in `dir`, with the timers a test's worker runs with, which keeps its process records in
    /// [`process_records_dir`].
    pub(super) async fn start(dir: &Path) -> Self {
        Self::start_with(dir, worker::Timers::FOR_TESTS).await
    }

    /// A worker for workspaces in `dir`, as [`Hub::start`] makes, that runs with `timers`.
    pub(super) async fn start_with(dir: &Path, timers: worker::Timers) -> Self {
        let config = worker::Config { timers, ..config(dir, None) };
        let super::super::Worker { read, write, running } = start_worker(config);
        let (sending, frames) = mpsc::unbounded_channel();
        // Read as they come, so the worker never waits on the test.
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = sending.send(frame::decode(line.as_bytes()).unwrap());
            }
        });
        let (replies, output, updates, next) = (HashMap::new(), Vec::new(), Vec::new(), HashMap::new());
        let (searched, updates_searched, pictures) = (0, 0, Vec::new());
        let mut hub = Self {
            write,
            frames,
            running,
            calls: 0,
            replies,
            output,
            searched,
            updates,
            updates_searched,
            next,
            pictures,
        };
        hub.call(hello()).await;
        hub
    }

    /// Opens terminal `terminal` running `sh` in `dir`.
    pub(super) async fn open(&mut self, terminal: &str, dir: &Path) {
        let started = self.start_terminal(terminal, dir, SIZE).await;
        assert_eq!(started, WorkerOutcome::Response(WorkerResponse::Ok));
    }

    /// Asks the worker to start terminal `terminal` running `sh` in `dir`, with `dir` for its home, so it reads no
    /// one's profile, and with variables a parent Claude session and terminal apps set, as a hub started from inside
    /// them has, and a `GH_TOKEN`, as a worker started where one is set has.
    pub(super) async fn start_terminal(&mut self, terminal: &str, dir: &Path, size: TerminalSize) -> WorkerOutcome {
        let home = dir.to_string_lossy().into_owned();
        let env = [
            ("SHELL", "/bin/sh"),
            ("HOME", home.as_str()),
            ("PORT", "55000"),
            ("CLAUDECODE", "1"),
            ("TMUX", "/tmp/tmux-501/default,1,0"),
            ("GHOSTTY_RESOURCES_DIR", "/Applications/Ghostty.app/Contents/Resources/ghostty"),
            ("GH_TOKEN", "the-machines-own"),
        ];
        let env = env.iter().map(|(name, value)| ((*name).to_string(), (*value).to_string())).collect();
        let start = TerminalStart { terminal: terminal.into(), size, cwd: home, env, opened_at_ms: OPENED_AT_MS };
        self.call(WorkerRequest::TerminalStart(start)).await
    }

    /// What the worker lists as running, with when the hub opened each.
    pub(super) async fn listed(&mut self) -> Vec<(TerminalId, i64)> {
        let listed = self.call(WorkerRequest::RunningTerminals).await;
        let WorkerOutcome::Response(WorkerResponse::RunningTerminals(RunningTerminalsReply { terminals })) = listed
        else {
            panic!("the worker didn't list its terminals: {listed:?}");
        };
        terminals.into_iter().map(|running| (running.terminal, running.opened_at_ms)).collect()
    }

    pub(super) async fn shown(&mut self, terminal: &str) -> TerminalShown {
        let shown = self.call(WorkerRequest::TerminalShow(TerminalShow { terminal: terminal.into() })).await;
        let WorkerOutcome::Response(WorkerResponse::TerminalShown(shown)) = shown else {
            panic!("the terminal didn't say what it shows: {shown:?}");
        };
        shown
    }

    /// Types `line` and Return.
    pub(super) async fn type_in(&mut self, terminal: &str, line: &str) {
        let typed = self.call(input(terminal, &format!("{line}\r"))).await;
        assert_eq!(typed, WorkerOutcome::Response(WorkerResponse::Ok));
    }

    pub(super) async fn call(&mut self, request: WorkerRequest) -> WorkerOutcome {
        let id = self.calls;
        self.calls += 1;
        self.write.write_all(&frame::encode(&WorkerCall { id, request }).unwrap()).await.unwrap();
        self.until(|hub| hub.replies.remove(&id)).await
    }

    /// Until a program writes `text`, after what the last wait found, and then how many updates have come of the
    /// terminal, as there's one.
    pub(super) async fn until_output(&mut self, text: &str) -> UpdateIndex {
        self.until(|hub| {
            let found = hub.output[hub.searched..].windows(text.len()).position(|window| window == text.as_bytes())?;
            hub.searched += found + text.len();
            Some(hub.next.values().copied().max().unwrap_or(UpdateIndex::new(0)))
        })
        .await
    }

    /// The number a program writes after `label`, at the end of a line, as a PID it echoed.
    pub(super) async fn until_pid(&mut self, label: &str) -> u32 {
        loop {
            self.until_output(label).await;
            let start = self.searched;
            self.until(|hub| hub.output[start..].iter().position(|b| !b.is_ascii_digit())).await;
            let after = &self.output[start..];
            let digits = after.iter().take_while(|b| b.is_ascii_digit()).count();
            // The echo of what was typed has the label too, followed by what the shell works out.
            if digits > 0 && matches!(after[digits], b'\r' | b'\n') {
                return std::str::from_utf8(&after[..digits]).unwrap().parse().unwrap();
            }
        }
    }

    /// Until the worker sends `update`, after what the last wait for an update found. It can come before the reply to
    /// what caused it, as `Resized` does, since the worker replies once the terminal's thread is told.
    pub(super) async fn until_update(&mut self, update: &TerminalUpdate) {
        self.until(|hub| {
            let found = hub.updates[hub.updates_searched..].iter().position(|came| came == update)?;
            hub.updates_searched += found + 1;
            Some(())
        })
        .await;
    }

    /// Ends the hub's stream, as a hub that goes does, and waits for the worker to stop what it runs and exit. Every
    /// update but output, including those the worker sent as it stopped.
    pub(super) async fn finish(mut self) -> Vec<TerminalUpdate> {
        self.write.shutdown().await.unwrap();
        let rest = timeout(WAIT, async {
            while let Some(frame) = self.frames.recv().await {
                self.keep(frame);
            }
        });
        rest.await.expect("the worker to close its stream");
        timeout(WAIT, self.running).await.expect("the worker to exit").unwrap();
        self.updates
    }

    /// Until `found` finds something in what has come, keeping each frame as it comes.
    /// Until `found` says the pictures so far have what was waited for.
    pub(super) async fn until_pictures(&mut self, mut found: impl FnMut(&[(TerminalPicture, Instant)]) -> bool) {
        self.until(|hub| found(&hub.pictures).then_some(())).await;
    }

    async fn until<T>(&mut self, mut found: impl FnMut(&mut Self) -> Option<T>) -> T {
        timeout(WAIT, async {
            loop {
                if let Some(found) = found(self) {
                    return found;
                }
                let frame = self.frames.recv().await.expect("the worker went on");
                self.keep(frame);
            }
        })
        .await
        .unwrap_or_else(|_| panic!("what was waited for never came: {:?}", String::from_utf8_lossy(&self.output)))
    }

    fn keep(&mut self, frame: WorkerFrame) {
        match frame {
            WorkerFrame::Reply { id, outcome } => {
                self.replies.insert(id, outcome);
            }
            WorkerFrame::Terminal { terminal, at, update } => {
                let next = self.next.entry(terminal).or_insert(UpdateIndex::new(0));
                assert_eq!(at, *next, "an update came out of order: {update:?}");
                *next = at.next();
                match update {
                    TerminalUpdate::Output { output } => self.output.extend_from_slice(output.as_slice()),
                    update => self.updates.push(update),
                }
            }
            WorkerFrame::TerminalPicture { picture, .. } => self.pictures.push((picture, Instant::now())),
            _ => {}
        }
    }
}

/// Where the worker [`Hub::start`] starts for workspaces in `dir` records what its terminals run.
pub(super) fn process_records_dir(dir: &Path) -> PathBuf {
    config(dir, None).state_dir.join(AGENTS_DIR)
}

pub(super) fn refused(outcome: &WorkerOutcome, code: ErrorCode) -> bool {
    matches!(outcome, WorkerOutcome::Error(error) if error.code == code)
}

pub(super) fn input(terminal: &str, typed: &str) -> WorkerRequest {
    let terminal: TerminalId = terminal.into();
    WorkerRequest::TerminalInput(TerminalInput { terminal, input: typed.as_bytes().into() })
}

pub(super) fn contains(bytes: &[u8], text: &str) -> bool {
    bytes.windows(text.len()).any(|window| window == text.as_bytes())
}
