//! The thread that holds the engine and hears one recording at a time.

use std::sync::Arc;
use std::time::Duration;

use flume::RecvTimeoutError;

use super::buffer::Buffer;
use super::capture::{self, Capture, Open};
use super::hearing;
use super::recording::{Cancelled, Recording, Update};
use crate::{AudioIn, DictationError, Engine, EngineError, Hints, Language};

/// What loads the engine, each time it's needed after it was let go.
type Load = Box<dyn FnMut() -> Result<Box<dyn Engine>, EngineError> + Send>;

/// Dictation on this device: a thread that loads the engine when a recording needs it, hears recordings one at a
/// time, and lets go of the engine once it has been unused for a while. The engine never leaves the thread, since
/// Core ML's models can't be shared between threads. Dropping this ends the thread once any recording it's hearing
/// has ended.
pub struct Dictation {
    requests: flume::Sender<Request>,
}

/// A recording with a copy of its audio, as [`Dictation::record_copying`] starts one.
pub struct Copied {
    pub recording: Recording,
    pub updates: flume::Receiver<Update>,
    /// Its audio, at [`crate::SAMPLE_RATE`].
    pub audio: flume::Receiver<Vec<f32>>,
}

enum Request {
    Record(Asked),
    Prepare(flume::Sender<Result<(), EngineError>>),
}

/// A recording asked for: where its audio comes from, the channel that brings it and whatever else reaches it, and
/// where what's heard of it goes.
struct Asked {
    audio: AudioIn,
    hints: Hints,
    language: Language,
    input: Buffer,
    updates: flume::Sender<Update>,
    cancelled: Cancelled,
}

impl Dictation {
    /// Starts the thread, which calls `load` the first time a recording needs the engine, and again after letting go
    /// of it, once it has gone `keep_for` without a recording.
    pub fn start(
        load: impl FnMut() -> Result<Box<dyn Engine>, EngineError> + Send + 'static,
        keep_for: Duration,
    ) -> std::io::Result<Self> {
        Self::with_capture(load, keep_for, Arc::new(capture::microphone))
    }

    pub(super) fn with_capture(
        load: impl FnMut() -> Result<Box<dyn Engine>, EngineError> + Send + 'static,
        keep_for: Duration,
        capture: Open,
    ) -> std::io::Result<Self> {
        let (requests, asked) = flume::unbounded();
        let load: Load = Box::new(load);
        std::thread::Builder::new()
            .name("dictation".to_string())
            .spawn(move || run(load, &asked, keep_for, &capture))?;
        Ok(Self { requests })
    }

    /// Starts a recording, which hears `audio` in `language`, biased toward `hints`, until it's stopped or cancelled,
    /// and what's heard of it. Once an earlier recording has finished, the microphone is opened first, and then the
    /// engine loaded if it isn't. Audio awaiting recognition is bounded; exceeding the buffer ends capture with [`DictationError::AudioBacklog`].
    pub fn record(&self, audio: AudioIn, hints: Hints, language: Language) -> (Recording, flume::Receiver<Update>) {
        self.record_to(Buffer::new(None), audio, hints, language)
    }

    /// Starts a recording, as [`Dictation::record`] does, and gives a copy of all its audio too, as it comes: what the
    /// microphone gave before the recording was stopped, cancelled or failed, whether or not the engine got to it. The
    /// channel closes once the last of it has come.
    pub fn record_copying(&self, audio: AudioIn, hints: Hints, language: Language) -> Copied {
        let (copy, copied) = flume::unbounded();
        let (recording, updates) = self.record_to(Buffer::new(Some(copy)), audio, hints, language);
        Copied { recording, updates, audio: copied }
    }

    fn record_to(
        &self,
        input: Buffer,
        audio: AudioIn,
        hints: Hints,
        language: Language,
    ) -> (Recording, flume::Receiver<Update>) {
        let (updates, read) = flume::unbounded();
        let cancelled = Cancelled::default();
        let recording = Recording::new(input.clone(), audio, cancelled.clone());
        let asked = Asked { audio, hints, language, input, updates, cancelled };
        if let Err(flume::SendError(Request::Record(asked))) = self.requests.send(Request::Record(asked)) {
            let _ = asked.updates.send(Update::Failed(DictationError::Engine(stopped())));
        }
        (recording, read)
    }

    /// Loads the engine now, if it isn't loaded, and says how that went, such as once its files are downloaded, so the
    /// first recording doesn't wait. A recording being heard is heard first.
    pub fn prepare(&self) -> flume::Receiver<Result<(), EngineError>> {
        let (answer, read) = flume::bounded(1);
        if let Err(flume::SendError(Request::Prepare(answer))) = self.requests.send(Request::Prepare(answer)) {
            let _ = answer.send(Err(stopped()));
        }
        read
    }
}

/// What a recording or preparing is told once the thread has gone, as when it couldn't be started.
fn stopped() -> EngineError {
    EngineError("dictation stopped".to_string())
}

fn run(mut load: Load, asked: &flume::Receiver<Request>, keep_for: Duration, capture: &Open) {
    let mut engine = None;
    loop {
        match asked.recv_timeout(keep_for) {
            Ok(Request::Record(asked)) => {
                let (updates, cancelled) = (asked.updates.clone(), asked.cancelled.clone());
                let ended = match record(&mut engine, &mut load, asked, capture) {
                    Ok(Some(ending)) => Update::Ended(ending),
                    Ok(None) => continue,
                    Err(e) => Update::Failed(e),
                };
                // Nothing more is said of a cancelled recording, even of how it failed.
                cancelled.send_unless_set(&updates, ended);
            }
            Ok(Request::Prepare(answer)) => {
                let _ = answer.send(loaded(&mut engine, &mut load).map(|_| ()));
            }
            Err(RecvTimeoutError::Timeout) => engine = None,
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Hears one recording, until it's stopped, cancelled, or fails. `None` when it was cancelled, which can be while it
/// waited behind another, or for the engine to load.
fn record(
    engine: &mut Option<Box<dyn Engine>>,
    load: &mut Load,
    asked: Asked,
    capture: &Open,
) -> Result<Option<crate::Ending>, DictationError> {
    let Asked { audio, hints, language, input, updates, cancelled } = asked;
    if cancelled.is_set() {
        return Ok(None);
    }
    // This guard releases capture on an engine error as well. Stop and cancellation wake capture directly even
    // while this thread is blocked in the model.
    let _microphone = match audio {
        AudioIn::Microphone => {
            let (capture, device) = Capture::start(input.clone(), capture.clone())?;
            if let Some(device) = device {
                cancelled.send_unless_set(&updates, Update::Opened(device));
            }
            Some(capture)
        }
        #[cfg(any(test, feature = "testing"))]
        AudioIn::Fed => None,
    };
    let engine = loaded(engine, load)?;
    if cancelled.is_set() {
        return Ok(None);
    }
    hearing::hear(engine, &hints, language, &input, &updates, &cancelled)
}

/// The engine, loaded if it isn't. One that fails to load is tried again next time.
fn loaded<'e>(engine: &'e mut Option<Box<dyn Engine>>, load: &mut Load) -> Result<&'e dyn Engine, EngineError> {
    let ready = match engine.take() {
        Some(ready) => ready,
        None => load()?,
    };
    Ok(&**engine.insert(ready))
}
