//! A stand-in for an agent's CLI that replays one process of a capture through the adapter's own process code: a
//! script the adapter runs as the CLI, which hands its arguments, stdin and stdout to a replayer here. The replayer
//! writes what the CLI wrote, and at each line the adapter wrote in the capture, waits until it writes the same line
//! again, but for the IDs it makes afresh each time, so the adapter hears each line at the point it did.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use super::capture::Entry;
use super::lines::{Lines, asked_by_itself, ids_in};
use super::{executable, quoted};

/// Hands its arguments over as a file, and its stdin and stdout over FIFOs. The copy of stdin ignores SIGTERM, which
/// stopping the agent sends its whole group, so what the adapter wrote just before is still heard. It ends once the
/// adapter closes the CLI's stdin, and leaves stderr alone, so the adapter isn't kept waiting for it. A background
/// list's stdin is `/dev/null` unless it's redirected, so it reads the script's own from a copy. The script leaves
/// once the replayer has written all it has, with the code the replayer leaves it.
const SCRIPT: &str = r#"printf '%s\0' "$@" >"$DIR/args"
exec 3<&0
trap '' TERM
cat <&3 >"$DIR/to-cli" 2>/dev/null &
trap - TERM
exec 3<&-
cat "$DIR/from-cli"
exit "$(cat "$DIR/code" 2>/dev/null || echo 0)"
"#;

/// How long the replayer waits for the adapter to write the line it's waiting for, as long as a replay's step waits
/// for the adapter, so a replay that's gone wrong ends rather than hanging.
pub const QUIET: Duration = Duration::from_secs(10);

/// One process's stand-in, replaying until the adapter has gone.
pub struct StandIn {
    pub program: PathBuf,
    to_cli: PathBuf,
    from_cli: PathBuf,
    /// Set once the replayer has opened both FIFOs, which it waits for the stand-in to open first.
    opened: Arc<AtomicBool>,
    replayer: JoinHandle<Vec<String>>,
}

impl StandIn {
    /// A stand-in in `dir`, which must be empty, replaying `process` by `lines`'s rules, which sends the name of each
    /// [`Entry::Saw`] to `saw` once it has written what came before it.
    pub fn start(
        dir: &Path,
        process: Vec<Entry>,
        lines: &'static dyn Lines,
        saw: UnboundedSender<String>,
    ) -> io::Result<StandIn> {
        let to_cli = dir.join("to-cli");
        let from_cli = dir.join("from-cli");
        for fifo in [&to_cli, &from_cli] {
            if !Command::new("mkfifo").arg(fifo).status()?.success() {
                return Err(io::Error::other(format!("mkfifo {} failed", fifo.display())));
            }
        }
        let program = executable(dir, "stand-in", &format!("#!/bin/sh\nDIR={}\n{SCRIPT}", quoted(dir)))?;
        let opened = Arc::new(AtomicBool::new(false));
        let replay = Replay { dir: dir.to_path_buf(), lines, saw, opened: opened.clone() };
        let replayer = std::thread::spawn(move || replay.run(process));
        Ok(StandIn { program, to_cli, from_cli, opened, replayer })
    }

    /// What the adapter did that the capture didn't have where it was, once the stand-in has gone. Empty when it did
    /// just what the capture has.
    pub fn finish(self) -> Vec<String> {
        // A stand-in that never ran, or was stopped before it opened the FIFOs, leaves the replayer waiting to open
        // them, so this opens the other ends. Each open waits for the replayer's, so it's done on a thread of its
        // own, which ends as soon as the replayer has opened both.
        if !self.opened.load(Ordering::SeqCst) {
            let (to_cli, from_cli) = (self.to_cli.clone(), self.from_cli.clone());
            std::thread::spawn(move || {
                let _ = OpenOptions::new().write(true).open(to_cli);
                let _ = File::open(from_cli);
            });
        }
        self.replayer.join().unwrap_or_else(|_| vec!["the replayer panicked".into()])
    }
}

struct Replay {
    dir: PathBuf,
    lines: &'static dyn Lines,
    saw: UnboundedSender<String>,
    opened: Arc<AtomicBool>,
}

impl Replay {
    fn run(self, process: Vec<Entry>) -> Vec<String> {
        // Each open waits for the stand-in to open the other end.
        let adapter = File::open(self.dir.join("to-cli"));
        let cli =
            adapter.and_then(|adapter| Ok((adapter, OpenOptions::new().write(true).open(self.dir.join("from-cli"))?)));
        let (adapter, cli) = match cli {
            Ok(opened) => opened,
            Err(e) => return vec![format!("opening the stand-in's FIFOs: {e}")],
        };
        self.opened.store(true, Ordering::SeqCst);
        let (sender, heard) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(adapter).lines() {
                let Ok(line) = line else { return };
                let value = serde_json::from_str(&line).unwrap_or(Value::String(line));
                if sender.send(value).is_err() {
                    return;
                }
            }
        });
        let mut replayer = Replayer::new(&process, self.lines, cli, heard);
        for entry in process {
            match entry {
                Entry::Launch { args, .. } => replayer.launched_with(&args, &self.dir.join("args")),
                Entry::FromCli(line) if replayer.answered_by_itself(&line) => {}
                Entry::FromCli(line) => replayer.write(&line.to_string()),
                Entry::FromCliText(text) => replayer.write(&text),
                Entry::ToCli(line) if self.lines.by_itself(&line) => {}
                Entry::ToCli(line) => replayer.expect(&line),
                Entry::Saw(name) => {
                    // The driver may have stopped waiting, as when a step failed, and then nobody's listening.
                    let _ = self.saw.send(name);
                }
                Entry::Exit { code, requested: false, .. } => {
                    let _ = std::fs::write(self.dir.join("code"), code.unwrap_or(1).to_string());
                    replayer.cli = None;
                }
                Entry::Exit { .. } => {}
            }
        }
        replayer.drain()
    }
}

struct Replayer {
    /// Gone once the CLI it stands in for has exited, or the stand-in has.
    cli: Option<File>,
    heard: mpsc::Receiver<Value>,
    lines: &'static dyn Lines,
    /// What the adapter wrote that hasn't been matched with a line of the capture yet.
    early: Vec<Value>,
    /// Each ID the adapter made in the capture, with the one it made for the same line now.
    ids: Vec<(String, String)>,
    /// The CLI's answers in the capture to the requests the adapter makes by itself, by the request's kind, in order,
    /// and the last one given, for a request the capture had fewer of.
    answers: HashMap<String, (Vec<Value>, Option<Value>)>,
    /// The IDs of those requests in the capture, whose answers are given when asked rather than in their place.
    asked_by_itself: Vec<String>,
    problems: Vec<String>,
}

impl Replayer {
    fn new(process: &[Entry], lines: &'static dyn Lines, cli: File, heard: mpsc::Receiver<Value>) -> Replayer {
        let mut replayer = Replayer {
            cli: Some(cli),
            heard,
            lines,
            early: Vec::new(),
            ids: Vec::new(),
            answers: HashMap::new(),
            asked_by_itself: Vec::new(),
            problems: Vec::new(),
        };
        let mut kinds = HashMap::new();
        for (kind, id) in asked_by_itself(lines, process) {
            kinds.insert(id.to_string(), kind);
            replayer.asked_by_itself.push(id.to_string());
        }
        for entry in process {
            if let Entry::FromCli(line) = entry
                && let Some(kind) = lines.answers(line).and_then(|id| kinds.get(id))
            {
                replayer.answers.entry(kind.clone()).or_default().0.push(line.clone());
            }
        }
        replayer
    }

    fn launched_with(&mut self, captured: &[String], file: &Path) {
        let now = std::fs::read(file).unwrap_or_default();
        let now: Vec<_> =
            String::from_utf8_lossy(&now).split('\0').filter(|arg| !arg.is_empty()).map(String::from).collect();
        if now != captured {
            self.problems
                .push(format!("the adapter launched the CLI with {now:?}, and in the capture with {captured:?}"));
        }
    }

    fn answered_by_itself(&self, line: &Value) -> bool {
        self.lines.answers(line).is_some_and(|id| self.asked_by_itself.iter().any(|asked| asked == id))
    }

    /// Writes `line` as the CLI, with each ID the adapter made in the capture changed to the one it made now. A write
    /// fails once the stand-in has gone, as when the driver stopped it, and then nothing more is written.
    fn write(&mut self, line: &str) {
        let Some(cli) = &mut self.cli else { return };
        let mut line = line.to_string();
        for (captured, now) in &self.ids {
            line = line.replace(captured.as_str(), now);
        }
        line.push('\n');
        if cli.write_all(line.as_bytes()).and_then(|()| cli.flush()).is_err() {
            self.cli = None;
        }
    }

    /// Waits until the adapter writes `captured` again, answering what it asks by itself meanwhile. A line of the same
    /// kind that says something else stands in for it, and is a problem.
    fn expect(&mut self, captured: &Value) {
        let kind = self.lines.kind(captured);
        loop {
            let same = self.early.iter().position(|line| self.same(captured, line));
            let at = same.or_else(|| self.early.iter().position(|line| self.lines.kind(line) == kind));
            if let Some(at) = at {
                let line = self.early.remove(at);
                if same.is_none() {
                    let (now, then) = (cut(&line), cut(captured));
                    self.problems.push(format!("the adapter wrote {kind} as {now}, and in the capture as {then}"));
                }
                self.match_ids(captured, &line);
                return;
            }
            let Ok(line) = self.heard.recv_timeout(QUIET) else {
                self.problems.push(format!("the adapter never wrote {kind}"));
                return;
            };
            self.heard(line);
        }
    }

    /// Whether `line` is `captured` again, but for the IDs the adapter makes afresh.
    fn same(&self, captured: &Value, line: &Value) -> bool {
        let paths = self.lines.fresh_ids(captured);
        self.lines.kind(captured) == self.lines.kind(line) && without(captured, paths) == without(line, paths)
    }

    fn heard(&mut self, line: Value) {
        if self.lines.by_itself(&line) {
            let (captured, last) = self.answers.entry(self.lines.kind(&line)).or_default();
            let next = if captured.is_empty() { last.clone() } else { Some(captured.remove(0)) };
            last.clone_from(&next);
            let answer = self.lines.answer(next.as_ref(), &line);
            self.write(&answer.to_string());
        } else {
            self.early.push(line);
        }
    }

    fn match_ids(&mut self, captured: &Value, now: &Value) {
        let paths = self.lines.fresh_ids(captured);
        for (captured, now) in ids_in(captured, paths).into_iter().zip(ids_in(now, paths)) {
            if captured != now {
                self.ids.push((captured.to_string(), now.to_string()));
            }
        }
    }

    /// Answers what the adapter asks by itself until the stand-in has gone, then says what it wrote that the capture
    /// didn't have.
    fn drain(mut self) -> Vec<String> {
        while let Ok(line) = self.heard.recv_timeout(QUIET) {
            self.heard(line);
        }
        let extra: Vec<_> =
            self.early.iter().map(|line| format!("the adapter also wrote {}", self.lines.kind(line))).collect();
        self.problems.extend(extra);
        self.problems
    }
}

/// `line`, with what's at each of `paths` taken out.
fn without(line: &Value, paths: &[&[&str]]) -> Value {
    let mut line = line.clone();
    for path in paths {
        let Some((last, parents)) = path.split_last() else { continue };
        if let Some(Value::Object(object)) = parents.iter().try_fold(&mut line, |value, key| value.get_mut(*key)) {
            object.remove(*last);
        }
    }
    line
}

/// A line, cut short enough to read in a report.
fn cut(line: &Value) -> String {
    let text = line.to_string();
    match text.char_indices().nth(300) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}
