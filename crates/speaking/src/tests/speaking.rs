//! What the thread does with the voice and the output: when it opens and closes the output, what a failure stops, and
//! what a stop counts as heard. A fake voice and output stand in for Phonon and the speakers, and the test says when the
//! output reaches each mark.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use phonon::{PhononError, Speed};
use sound::{Played, SpeakersError};

use crate::speaking::parts::{Output, Speech, Voice};
use crate::{Speaking, TextId, Update};

/// Long enough for anything a test waits on, which takes milliseconds.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Longer than any test, so the output is closed only when a test means it to be.
const KEEP: Duration = Duration::from_secs(600);

const REPLY: TextId = TextId(1);
const NEXT: TextId = TextId(2);

/// A voice that makes a sample of silence for each byte of what it says, in two pieces, and tells the test what it
/// said at which speed.
struct FakeVoice {
    speed: Speed,
    spoken: flume::Sender<(String, Speed)>,
}

impl Voice for FakeVoice {
    fn sample_rate(&self) -> u32 {
        24_000
    }

    fn speak(&self, text: &str) -> Result<Speech, PhononError> {
        let _ = self.spoken.send((text.to_string(), self.speed));
        let half = vec![0.0; text.len() / 2];
        Ok(Box::new([Ok(half.clone()), Ok(half)].into_iter()))
    }

    fn set_speed(&mut self, speed: Speed) -> Result<(), PhononError> {
        self.speed = speed;
        Ok(())
    }
}

/// What a test sees of the fake output: each mark it's given, and its closing.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Mark(u64),
    Closed,
}

/// The output the thread has open, as the test drives it.
#[derive(Default)]
struct Device {
    /// The marks given that the test hasn't had it reach.
    given: Vec<u64>,
    /// How many of them it reaches as it's stopped, as a device can just before it stops.
    reached_as_stopped: usize,
    tell: Option<flume::Sender<Played>>,
}

/// An output the test says when to reach each mark, and fails when the test says.
struct FakeOutput {
    device: Arc<Mutex<Device>>,
    seen: flume::Sender<Seen>,
    played: flume::Receiver<Played>,
}

impl Output for FakeOutput {
    fn play(&mut self, _: &[f32]) {}

    fn mark(&mut self, mark: u64) {
        self.device.lock().given.push(mark);
        let _ = self.seen.send(Seen::Mark(mark));
    }

    fn stop(&mut self) {
        let mut device = self.device.lock();
        let count = device.reached_as_stopped;
        let reached: Vec<u64> = device.given.drain(..count).collect();
        for mark in reached {
            let _ = device.tell.as_ref().unwrap().send(Played::Reached(mark));
        }
        device.given.clear();
    }

    fn played(&self) -> &flume::Receiver<Played> {
        &self.played
    }
}

impl Drop for FakeOutput {
    fn drop(&mut self) {
        *self.device.lock() = Device::default();
        let _ = self.seen.send(Seen::Closed);
    }
}

/// The thread, with a fake voice and output, and what the test sees of them.
struct Harness {
    speaking: Speaking,
    updates: flume::Receiver<Update>,
    device: Arc<Mutex<Device>>,
    seen: flume::Receiver<Seen>,
    loads: Arc<AtomicUsize>,
    opens: Arc<AtomicUsize>,
    /// What the voice said, and at which speed.
    spoken: flume::Receiver<(String, Speed)>,
}

impl Harness {
    fn start(close_after: Duration) -> Self {
        let device = Arc::new(Mutex::new(Device::default()));
        let (seeing, seen) = flume::unbounded();
        let (loads, opens) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let loaded = loads.clone();
        let (saying, spoken) = flume::unbounded();
        let load = move || {
            loaded.fetch_add(1, Ordering::SeqCst);
            Ok(FakeVoice { speed: Speed::default(), spoken: saying.clone() })
        };
        let (opened, opening) = (opens.clone(), device.clone());
        let open = move |_rate| {
            opened.fetch_add(1, Ordering::SeqCst);
            let (tell, played) = flume::unbounded();
            opening.lock().tell = Some(tell);
            Ok(FakeOutput { device: opening.clone(), seen: seeing.clone(), played })
        };
        let speaking = Speaking::start_with(load, open, close_after).unwrap();
        let updates = speaking.updates();
        Self { speaking, updates, device, seen, loads, opens, spoken }
    }

    /// Reads all of `text` as `id`.
    fn read(&self, id: TextId, text: &str) {
        self.speaking.say(id, 0, text);
        self.speaking.end(id);
    }

    /// Waits until the output has been given `count` more marks.
    fn marks(&self, count: usize) {
        for _ in 0..count {
            let seen = self.seen.recv_timeout(TIMEOUT).unwrap();
            assert!(matches!(seen, Seen::Mark(_)), "{seen:?}");
        }
    }

    /// Waits until the output is closed.
    fn closed(&self) {
        assert_eq!(self.seen.recv_timeout(TIMEOUT), Ok(Seen::Closed));
    }

    /// Has the output reach the first `count` marks it was given.
    fn reach(&self, count: usize) {
        let mut device = self.device.lock();
        let reached: Vec<u64> = device.given.drain(..count).collect();
        for mark in reached {
            let _ = device.tell.as_ref().unwrap().send(Played::Reached(mark));
        }
    }

    /// Has the output fail.
    fn fail(&self) {
        let device = self.device.lock();
        let _ = device.tell.as_ref().unwrap().send(Played::Failed(SpeakersError::Missing));
    }

    /// Updates, as text, until one is `last`.
    fn until(&self, last: &str) -> Vec<String> {
        let mut seen = Vec::new();
        while seen.last().is_none_or(|update| update != last) {
            match self.updates.recv_timeout(TIMEOUT) {
                Ok(update) => seen.push(format!("{update:?}")),
                Err(e) => panic!("{e} waiting for {last}, after {seen:?}"),
            }
        }
        seen
    }
}

/// A text is heard as the output reaches each phrase's mark, and finishes with its last.
#[test]
fn a_text_is_heard_as_the_output_reaches_its_marks() {
    let harness = Harness::start(KEEP);
    harness.read(REPLY, "One. Two.");
    harness.marks(2);
    harness.reach(2);

    assert_eq!(
        harness.until("Finished { text: TextId(1) }"),
        [
            "Heard { text: TextId(1), through: 5 }",
            "Heard { text: TextId(1), through: 9 }",
            "Finished { text: TextId(1) }"
        ]
    );
}

/// A stop counts what the output reached just before it stopped, which the thread is told only after.
#[test]
fn a_stop_counts_what_was_reached_just_before_it() {
    let harness = Harness::start(KEEP);
    harness.read(REPLY, "One. Two. Three.");
    harness.marks(3);
    harness.device.lock().reached_as_stopped = 1;
    harness.speaking.stop();

    assert_eq!(
        harness.until("Stopped { cut: Some((TextId(1), 5)), never_started: [] }"),
        ["Heard { text: TextId(1), through: 5 }", "Stopped { cut: Some((TextId(1), 5)), never_started: [] }"]
    );
}

/// An output that failed while reading stops the reading and is closed, so the next text opens another.
#[test]
fn a_failure_stops_the_reading_and_closes_the_output() {
    let harness = Harness::start(KEEP);
    harness.read(REPLY, "One. Two.");
    harness.marks(2);
    harness.fail();
    assert_eq!(
        harness.until("Stopped { cut: Some((TextId(1), 0)), never_started: [] }"),
        ["Failed(Speakers(Missing))", "Stopped { cut: Some((TextId(1), 0)), never_started: [] }"]
    );
    harness.closed();

    harness.read(NEXT, "Three.");
    harness.marks(1);
    harness.reach(1);
    assert_eq!(
        harness.until("Finished { text: TextId(2) }"),
        ["Heard { text: TextId(2), through: 6 }", "Finished { text: TextId(2) }"]
    );
    assert_eq!(harness.opens.load(Ordering::SeqCst), 2);
}

/// An output that fails with nothing being read is only closed, and the next text opens it again.
#[test]
fn a_failure_with_nothing_being_read_only_closes_the_output() {
    let harness = Harness::start(KEEP);
    harness.read(REPLY, "One.");
    harness.marks(1);
    harness.reach(1);
    harness.until("Finished { text: TextId(1) }");
    harness.fail();
    harness.closed();

    harness.read(NEXT, "Two.");
    harness.marks(1);
    harness.reach(1);
    assert_eq!(
        harness.until("Finished { text: TextId(2) }"),
        ["Heard { text: TextId(2), through: 4 }", "Finished { text: TextId(2) }"],
        "nothing of the failure"
    );
}

/// The output is closed once nothing has played for a while, so the device isn't held open, and opened again for the
/// next text.
#[test]
fn the_output_closes_once_nothing_has_played_for_a_while() {
    let harness = Harness::start(Duration::from_millis(20));
    harness.read(REPLY, "One.");
    harness.marks(1);
    harness.reach(1);
    harness.until("Finished { text: TextId(1) }");
    harness.closed();

    harness.read(NEXT, "Two.");
    harness.marks(1);
    assert_eq!(harness.opens.load(Ordering::SeqCst), 2);
}

/// A text with nothing to say finishes at once, without loading the voice or opening the output.
#[test]
fn a_text_with_nothing_to_say_needs_no_voice_or_output() {
    let harness = Harness::start(KEEP);
    harness.read(REPLY, "");
    harness.read(NEXT, "|---|");

    assert_eq!(
        harness.until("Finished { text: TextId(2) }"),
        ["Finished { text: TextId(1) }", "Heard { text: TextId(2), through: 5 }", "Finished { text: TextId(2) }"]
    );
    assert_eq!((harness.loads.load(Ordering::SeqCst), harness.opens.load(Ordering::SeqCst)), (0, 0));
}

/// A speed chosen before the voice is loaded is the one it speaks at once loaded, and one chosen later is the one it
/// speaks the next phrase at.
#[test]
fn the_voice_speaks_at_the_speed_last_chosen() {
    let harness = Harness::start(KEEP);
    harness.speaking.set_speed(Speed::Faster);
    harness.read(REPLY, "One.");
    harness.marks(1);
    assert_eq!(harness.spoken.recv_timeout(TIMEOUT), Ok(("One.".to_string(), Speed::Faster)));

    harness.speaking.set_speed(Speed::Fastest);
    harness.read(NEXT, "Two.");
    harness.marks(1);
    assert_eq!(harness.spoken.recv_timeout(TIMEOUT), Ok(("Two.".to_string(), Speed::Fastest)));
}
