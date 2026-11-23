//! Hearing a clip as the app hears a recording: through `Dictation`, cut at its pauses, with live text as it's said
//! and each sentence's final text once it has ended.
//!
//! The clip is fed 10 ms at a time, as the microphone gives it, and each 10 ms only once the engine has heard the
//! last, so the engine is never behind, as it isn't in the app on a Mac fast enough to keep up. The run takes only as
//! long as the engine does, rather than as long as the clip, and the time from the end of the clip to its last final
//! text is what a person would wait after letting go of the key.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use dictation::{AudioIn, Dictation, Engine, EngineError, Heard, Hints, Language, Listening, Samples, Update};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// How much audio the microphone gives at a time: 10 ms.
const BUFFER: usize = Samples::from_millis(10).0;

/// How long dictation keeps the engine without a recording: longer than any run, so one waiting on a download between
/// sets doesn't load the model again and put the time it takes into a clip's.
const KEPT_FOR: Duration = Duration::from_hours(24);

/// How long the engine has to hear 10 ms before the clip is given up on, as when it has stopped.
const STUCK: Duration = Duration::from_secs(120);

/// What was written of a clip.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Written {
    /// All the live text, as the engine gave it.
    pub(crate) live: String,
    /// The final texts, one after another.
    #[serde(rename = "final")]
    pub(crate) final_text: String,
    /// How long the engine took over all of it.
    pub(crate) engine_ms: u64,
    /// How long from the end of the clip to its last final text.
    pub(crate) after_ms: u64,
}

/// What the engine has heard of the recording it's hearing, which the engine's thread notes as it hears.
#[derive(Default)]
struct Noted {
    live: String,
    /// How many of the recording's samples it has heard live.
    heard: usize,
}

/// Dictation's thread, with an engine that notes what it hears.
pub(crate) struct Hearer {
    dictation: Dictation,
    noted: Arc<Mutex<Noted>>,
    heard: flume::Receiver<()>,
}

impl Hearer {
    /// Starts dictation's thread, which loads the engine `load` gives when the first clip is heard.
    pub(crate) fn start(
        mut load: impl FnMut() -> Result<Box<dyn Engine>, EngineError> + Send + 'static,
    ) -> anyhow::Result<Self> {
        let noted = Arc::new(Mutex::new(Noted::default()));
        let (tell, heard) = flume::unbounded();
        let noting = noted.clone();
        let dictation = Dictation::start(
            move || {
                Ok(Box::new(Noting { engine: load()?, noted: noting.clone(), heard: tell.clone() }) as Box<dyn Engine>)
            },
            KEPT_FOR,
        )
        .context("couldn't start dictation's thread")?;
        Ok(Self { dictation, noted, heard })
    }

    /// Loads the engine and hears a second of quiet with it, so the first clip's time includes neither loading it nor
    /// Core ML's first run of each encoder, which is slower than the rest.
    pub(crate) fn prepare(&self) -> anyhow::Result<()> {
        let answer = self.dictation.prepare().recv().context("dictation's thread stopped")?;
        answer.context("the engine didn't load")?;
        self.hear(&vec![0.0; Samples::SECOND.0], Hints::default(), Language::default()).map(|_| ())
    }

    /// Hears `audio`, at the model's rate, in `language`, biased toward `hints`.
    pub(crate) fn hear(&self, audio: &[f32], hints: Hints, language: Language) -> anyhow::Result<Written> {
        *self.noted.lock() = Noted::default();
        while self.heard.try_recv().is_ok() {}
        let started = Instant::now();
        let (recording, updates) = self.dictation.record(AudioIn::Fed, hints, language);
        let mut final_text = Vec::new();
        let mut fed = 0;
        for buffer in audio.chunks(BUFFER) {
            recording.feed(buffer.to_vec());
            fed += buffer.len();
            let waiting = Instant::now();
            while self.noted.lock().heard < fed {
                take(&updates, &mut final_text)?;
                if self.heard.recv_timeout(Duration::from_millis(100)).is_err() && waiting.elapsed() > STUCK {
                    bail!("the engine heard nothing for {} s", STUCK.as_secs());
                }
            }
        }
        let stopped = Instant::now();
        recording.stop();
        loop {
            match updates.recv_timeout(STUCK).context("the recording didn't end")? {
                Update::Final(text) => final_text.push(text),
                Update::Hearing(_) => {}
                Update::Ended(_) => break,
                Update::Failed(error) => return Err(error).context("the recording failed"),
            }
        }
        let live = std::mem::take(&mut self.noted.lock().live);
        Ok(Written {
            live: live.trim().to_string(),
            final_text: final_text.join(" "),
            engine_ms: millis(started.elapsed()),
            after_ms: millis(stopped.elapsed()),
        })
    }
}

/// Takes the final texts that have come, and fails if the recording has.
fn take(updates: &flume::Receiver<Update>, final_text: &mut Vec<String>) -> anyhow::Result<()> {
    for update in updates.try_iter() {
        match update {
            Update::Final(text) => final_text.push(text),
            Update::Hearing(_) => {}
            Update::Ended(_) => bail!("the recording ended before it was stopped"),
            Update::Failed(error) => return Err(error).context("the recording failed"),
        }
    }
    Ok(())
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// An engine that notes what its recordings are heard as live, and how far into each it has heard.
struct Noting {
    engine: Box<dyn Engine>,
    noted: Arc<Mutex<Noted>>,
    heard: flume::Sender<()>,
}

impl Engine for Noting {
    fn longest_piece(&self) -> Samples {
        self.engine.longest_piece()
    }

    fn listen(&self, hints: &Hints, language: Language) -> Box<dyn Listening + '_> {
        Box::new(NotingListening { listening: self.engine.listen(hints, language), noting: self })
    }

    fn transcribe(&self, piece: &[f32], hints: &Hints, language: Language) -> Result<String, EngineError> {
        self.engine.transcribe(piece, hints, language)
    }
}

struct NotingListening<'e> {
    listening: Box<dyn Listening + 'e>,
    noting: &'e Noting,
}

impl NotingListening<'_> {
    fn note(&self, heard: &[Heard], samples: usize) {
        let mut noted = self.noting.noted.lock();
        noted.live.extend(heard.iter().map(|heard| heard.text.as_str()));
        noted.heard += samples;
        drop(noted);
        let _ = self.noting.heard.send(());
    }
}

impl Listening for NotingListening<'_> {
    fn hear(&mut self, audio: &[f32]) -> Result<Vec<Heard>, EngineError> {
        let heard = self.listening.hear(audio)?;
        self.note(&heard, audio.len());
        Ok(heard)
    }

    fn finish(&mut self) -> Result<Vec<Heard>, EngineError> {
        let heard = self.listening.finish()?;
        self.note(&heard, 0);
        Ok(heard)
    }

    fn heard_through(&self) -> Samples {
        self.listening.heard_through()
    }
}
