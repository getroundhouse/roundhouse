//! Reads a text out through the speakers, a phrase at a time, and prints each update as it comes, with when: to check
//! reading out by hand, since CI has neither the model nor speakers. `--stop-after <seconds>` stops it partway, to see
//! where it was cut.
//!
//! `cargo run -p speaking --release --example read -- <model directory> "<text>"`, where the directory holds the files
//! `phonon::MODEL` lists, as the app keeps them, by revision.

#![expect(clippy::unwrap_used, reason = "a tool run by hand, which stops at the first problem")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use phonon::{MODEL, Phonon};
use speaking::{Speaking, TextId, Update};

fn main() {
    let mut stop_after = None;
    let mut positional = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--stop-after" {
            stop_after = Some(Duration::from_secs_f64(args.next().unwrap().parse().unwrap()));
        } else {
            positional.push(arg);
        }
    }
    let [model_dir, text] = &positional[..] else { panic!("a model directory and a text") };
    let dir: PathBuf = MODEL.dir(model_dir.as_ref());
    let speaking = Speaking::start(move || Phonon::load(&dir)).unwrap();
    let updates = speaking.updates();
    let started = Instant::now();
    speaking.say(TextId(1), 0, text);
    speaking.end(TextId(1));
    let mut stopped = false;
    loop {
        let waited = match stop_after {
            Some(after) if !stopped => updates.recv_timeout(after.saturating_sub(started.elapsed())),
            _ => updates.recv().map_err(flume::RecvTimeoutError::from),
        };
        let at = started.elapsed().as_secs_f64();
        let update = match waited {
            Ok(update) => update,
            Err(flume::RecvTimeoutError::Timeout) => {
                println!("{at:6.2}s stop");
                speaking.stop();
                stopped = true;
                continue;
            }
            Err(flume::RecvTimeoutError::Disconnected) => panic!("the speaking thread ended"),
        };
        if let Update::Heard { through, .. } = &update {
            println!("{at:6.2}s {update:?} {:?}", &text[..*through]);
        } else {
            println!("{at:6.2}s {update:?}");
        }
        if matches!(update, Update::Finished { .. } | Update::Stopped { .. }) {
            return;
        }
    }
}
