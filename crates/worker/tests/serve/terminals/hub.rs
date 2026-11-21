//! The hub's end of a worker's stream, for the terminal tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::frame;
use protocol::model::TerminalId;
use protocol::paths::AGENTS_DIR;
use protocol::terminal::{Numbering, RowNumber, TerminalPicture, TerminalSize, TerminalUpdate, Typed};
use protocol::worker::{
    RunningTerminalsReply, TerminalStart, TerminalTyped, TerminalWatch, WorkerCall, WorkerFrame, WorkerOutcome,
    WorkerRequest, WorkerResponse,
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
    /// Every update of a terminal, which is its shell's end, in order.
    pub(super) updates: Vec<TerminalUpdate>,
    /// How many of `updates` the waits so far have found what they waited for in.
    updates_searched: usize,
    /// Every frame of a picture, in order, with when it came.
    pub(super) pictures: Vec<(TerminalPicture, Instant)>,
    /// Where the waits so far found what they waited for among the lines of `pictures`: the frame, and the line in it
    /// after what was found.
    shown_searched: (usize, usize),
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
        let mut hub = Self {
            write,
            frames,
            running,
            calls: 0,
            replies: HashMap::new(),
            updates: Vec::new(),
            updates_searched: 0,
            pictures: Vec::new(),
            shown_searched: (0, 0),
        };
        hub.call(hello()).await;
        hub
    }

    /// Opens terminal `terminal` running `sh` in `dir`, and watches it, so what it shows is sent as pictures.
    pub(super) async fn open(&mut self, terminal: &str, dir: &Path) {
        self.open_unwatched(terminal, dir).await;
        self.watch(terminal, true).await;
    }

    /// Opens terminal `terminal` running `sh` in `dir`, which sends nothing of what it shows until it's watched.
    pub(super) async fn open_unwatched(&mut self, terminal: &str, dir: &Path) {
        let started = self.start_terminal(terminal, dir, SIZE).await;
        assert_eq!(started, WorkerOutcome::Response(WorkerResponse::Ok));
    }

    /// Watches `terminal`, or stops.
    pub(super) async fn watch(&mut self, terminal: &str, watching: bool) {
        let watch = WorkerRequest::TerminalWatch(TerminalWatch { terminal: terminal.into(), watching });
        assert_eq!(self.call(watch).await, WorkerOutcome::Response(WorkerResponse::Ok));
    }

    /// Asks the worker to start terminal `terminal` running `sh` in `dir`, with `dir` for its home, so it reads no
    /// one's profile, and with variables terminal apps set, as a hub started from inside one has, and a `GH_TOKEN`, as a
    /// worker started where one is set has.
    pub(super) async fn start_terminal(&mut self, terminal: &str, dir: &Path, size: TerminalSize) -> WorkerOutcome {
        let home = dir.to_string_lossy().into_owned();
        let env = [
            ("SHELL", "/bin/sh"),
            ("HOME", home.as_str()),
            ("PORT", "55000"),
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
        let WorkerOutcome::Response(WorkerResponse::RunningTerminals(RunningTerminalsReply { terminals, .. })) = listed
        else {
            panic!("the worker didn't list its terminals: {listed:?}");
        };
        terminals.into_iter().map(|running| (running.terminal, running.opened_at_ms)).collect()
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

    /// Until a line of a picture of a watched terminal, after the one the last wait found, has `text`.
    pub(super) async fn until_shown(&mut self, text: &str) {
        self.until_line(|line| line.contains(text).then_some(())).await;
    }

    /// The number a program writes after `label`, at the end of a line, as a PID it echoed.
    pub(super) async fn until_pid(&mut self, label: &str) -> u32 {
        // The echo of what was typed has the label too, followed by what the shell works out.
        self.until_line(|line| line.rsplit_once(label)?.1.trim_end().parse().ok()).await
    }

    /// What `found` finds in a line of the pictures, after the one the last wait found, searching each line once, so a
    /// flood's pictures aren't searched again for each frame that comes.
    async fn until_line<T>(&mut self, mut found: impl FnMut(&str) -> Option<T>) -> T {
        self.until(|hub| {
            let (mut frame, mut from) = hub.shown_searched;
            while let Some((picture, _)) = hub.pictures.get(frame) {
                for (at, line) in lines(picture).iter().enumerate().skip(from) {
                    if let Some(found) = found(line) {
                        hub.shown_searched = (frame, at + 1);
                        return Some(found);
                    }
                }
                (frame, from) = (frame + 1, 0);
            }
            hub.shown_searched = (frame, 0);
            None
        })
        .await
    }

    /// Until the worker sends `update`, after what the last wait for an update found.
    pub(super) async fn until_update(&mut self, update: &TerminalUpdate) {
        self.until(|hub| {
            let found = hub.updates[hub.updates_searched..].iter().position(|came| came == update)?;
            hub.updates_searched += found + 1;
            Some(())
        })
        .await;
    }

    /// Ends the hub's stream, as a hub that goes does, and waits for the worker to stop what it runs and exit. Every
    /// update, including those the worker sent as it stopped.
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

    /// Keeps what comes for `long`, for what mustn't come.
    pub(super) async fn keep_for(&mut self, long: Duration) {
        let waited = waiting::task::for_what_mustnt_happen(long);
        tokio::pin!(waited);
        loop {
            tokio::select! {
                () = &mut waited => return,
                frame = self.frames.recv() => match frame {
                    Some(frame) => self.keep(frame),
                    None => return,
                },
            }
        }
    }

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
        .unwrap_or_else(|_| {
            let shown: Vec<_> = self.pictures.iter().rev().take(3).map(|(picture, _)| lines(picture)).collect();
            panic!("what was waited for never came, after the newest pictures, newest first: {shown:?}")
        })
    }

    fn keep(&mut self, frame: WorkerFrame) {
        match frame {
            WorkerFrame::Reply { id, outcome } => {
                self.replies.insert(id, outcome);
            }
            WorkerFrame::Terminal { update, .. } => self.updates.push(update),
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

/// Text typed into `terminal` as it is, as the bytes its program reads.
pub(super) fn input(terminal: &str, typed: &str) -> WorkerRequest {
    WorkerRequest::TerminalTyped(TerminalTyped { terminal: terminal.into(), typed: Typed::Text { text: typed.into() } })
}

/// The text of a picture's rows, as lines, each row that wraps joined to the next.
fn lines(picture: &TerminalPicture) -> Vec<String> {
    let mut lines = Vec::new();
    for run in &picture.rows {
        let mut line = String::new();
        for row in &run.rows {
            let row = row.row();
            line.extend(row.spans.iter().map(|span| span.text.as_str()));
            if !row.wraps {
                lines.push(std::mem::take(&mut line));
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

/// The numbering and number of a row the worker sent a picture of that ends with `text`, as a row the shell's prompt
/// can share, and the column `text` starts at.
pub(super) fn row_of(pictures: &[(TerminalPicture, Instant)], text: &str) -> Option<(Numbering, RowNumber, u16)> {
    pictures.iter().rev().find_map(|(picture, _)| {
        picture.rows.iter().find_map(|run| {
            let texts =
                run.rows.iter().map(|row| row.row().spans.iter().map(|span| span.text.clone()).collect::<String>());
            let (number, row) = (run.from.get()..).zip(texts).find(|(_, row)| row.ends_with(text))?;
            let col = u16::try_from(row.chars().count() - text.chars().count()).unwrap();
            Some((picture.numbering, RowNumber::new(number), col))
        })
    })
}
