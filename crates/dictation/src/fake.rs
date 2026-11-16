//! An engine that stands in for the model, for tests and the gallery, and audio for it to hear, so nothing needs a
//! recording of anyone's voice.
//!
//! Each word of a script is a tone of its own pitch, so the engine knows which word a burst of sound is from the
//! burst alone, wherever a piece starts. It hears a word live once half a second has passed since the word began, as
//! the model shows a word about that long after it's said, and gives a piece's final text as its words with the
//! first capitalised and a full stop at the end, so final text differs from live text as the model's does.
//!
//! The last word of a sentence is said quieter, as a voice falls at a sentence's end. Live, the engine gives the full
//! stop after it only with the next word, as the model does, since only then has it heard that the sentence ended,
//! and its final text has the full stop, with the next word capitalised.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::pauses::level_db;
use crate::{Engine, EngineError, Heard, Hints, Listening, Samples};

/// How long each word sounds.
const WORD: Samples = Samples::from_millis(300);

/// The quiet between words in a sentence, shorter than a pause that ends a piece.
const BETWEEN_WORDS: Samples = Samples::from_millis(120);

/// The quiet `/` stands for in what's said, longer than a pause that ends a piece.
const PAUSE: Samples = Samples::from_millis(1200);

/// How long after a word begins it's heard live, unless the engine is made to hold it back longer.
const HELD_BACK: Samples = Samples::from_millis(500);

/// How much audio each level is taken over, to find the bursts.
const STEP: usize = Samples::from_millis(10).0;

/// The first word's pitch, and how far apart each word's is, in Hz. Every word's is under 8 kHz up to the 70th.
const FIRST_PITCH: f32 = 300.0;
const PITCH_STEP: f32 = 100.0;

/// What a burst of a word is above, and the room's noise far below.
const BURST_DB: f32 = -30.0;

/// How loud a word is, and the last word of a sentence, which is said quieter. A burst is the quieter if its loudest
/// is nearer that, which a burst cut short in its first 10 ms, as it fades in, isn't.
const LOUDNESS: f32 = 0.3;
const FALLING: f32 = 0.1;

/// Stands in for the model, hearing the words of its script.
#[derive(Clone, Debug)]
pub struct FakeEngine {
    script: Vec<String>,
    longest: Samples,
    held_back: Samples,
    fails: bool,
    /// Whether its final text leaves off the full stop at its end, as the model's sometimes does.
    leaves_off_the_last_full_stop: bool,
    /// The hints the newest recording was started with, so tests can check what context a field passed.
    last_hints: Arc<Mutex<Hints>>,
}

/// A word heard in a burst.
struct Word {
    /// Where its burst begins.
    at: Samples,
    text: String,
    /// Whether it ends a sentence, by being said quieter.
    ends_a_sentence: bool,
}

impl FakeEngine {
    /// Hears the words of `script`, each of which [`FakeEngine::saying`] can say, and takes pieces of up to 15 s, as
    /// the model does.
    pub fn new(script: &[&str]) -> Self {
        let script = script.iter().map(|word| (*word).to_string()).collect();
        Self {
            script,
            longest: Samples::from_secs(15),
            held_back: HELD_BACK,
            fails: false,
            leaves_off_the_last_full_stop: false,
            last_hints: Arc::new(Mutex::new(Hints::default())),
        }
    }

    /// The hints given to the latest recording this engine (or a clone of it) heard.
    pub fn last_hints(&self) -> Hints {
        self.last_hints.lock().clone()
    }

    /// Takes pieces no longer than `longest`.
    #[must_use]
    pub fn with_longest_piece(self, longest: Samples) -> Self {
        Self { longest, ..self }
    }

    /// Hears each word live only once `held_back` has passed since it began, such as longer than a pause, as a model
    /// running slower than the audio comes would.
    #[must_use]
    pub fn holding_back(self, held_back: Samples) -> Self {
        Self { held_back, ..self }
    }

    /// Fails to transcribe any piece, as a model whose files went bad would.
    #[must_use]
    pub fn failing(self) -> Self {
        Self { fails: true, ..self }
    }

    /// Gives final text with no full stop at its end, even where a sentence ends, as the model sometimes does.
    #[must_use]
    pub fn leaving_off_the_last_full_stop(self) -> Self {
        Self { leaves_off_the_last_full_stop: true, ..self }
    }

    /// Audio of someone saying `said`: words of the script, between spaces, a full stop after a word that ends a
    /// sentence, and a pause long enough to end a piece wherever there's a `/`. Between words is a quiet room's noise.
    /// Panics on a word that isn't in the script.
    pub fn saying(&self, said: &str) -> Vec<f32> {
        let mut noise = Noise(1);
        let mut audio = Vec::new();
        let mut quiet = |audio: &mut Vec<f32>, length: Samples| audio.extend((0..length.0).map(|_| noise.next()));
        for (i, part) in said.split('/').enumerate() {
            if i > 0 {
                quiet(&mut audio, PAUSE);
            }
            for (j, word) in part.split_whitespace().enumerate() {
                if j > 0 {
                    quiet(&mut audio, BETWEEN_WORDS);
                }
                let (word, loudness) = word.strip_suffix('.').map_or((word, LOUDNESS), |word| (word, FALLING));
                let at = self.script.iter().position(|w| w.eq_ignore_ascii_case(word));
                let Some(at) = at else { panic!("{word:?} isn't in the script {:?}", self.script) };
                audio.extend(tone(FIRST_PITCH + PITCH_STEP * small(at), loudness));
            }
        }
        quiet(&mut audio, BETWEEN_WORDS);
        audio
    }

    /// The words of the bursts in `audio`, taking their casing from `hints` when a hint matches a word.
    fn words(&self, audio: &[f32], hints: &Hints) -> Vec<Word> {
        bursts(audio)
            .into_iter()
            .filter_map(|(start, end)| {
                let burst = &audio[start..end];
                let word = self.script.get(nearest((pitch(burst) - FIRST_PITCH) / PITCH_STEP))?;
                let text = hints
                    .words()
                    .iter()
                    .find(|hint| hint.eq_ignore_ascii_case(word))
                    .cloned()
                    .unwrap_or_else(|| word.to_lowercase());
                let loudest = burst.iter().fold(0.0_f32, |loudest, x| loudest.max(x.abs()));
                let ends_a_sentence = loudest < f32::midpoint(LOUDNESS, FALLING);
                Some(Word { at: Samples(start), text, ends_a_sentence })
            })
            .collect()
    }
}

impl Engine for FakeEngine {
    fn longest_piece(&self) -> Samples {
        self.longest
    }

    fn listen(&self, hints: &Hints) -> Box<dyn Listening + '_> {
        *self.last_hints.lock() = hints.clone();
        Box::new(FakeListening { engine: self, hints: hints.clone(), audio: Vec::new(), told: 0 })
    }

    fn transcribe(&self, piece: &[f32], hints: &Hints) -> Result<String, EngineError> {
        *self.last_hints.lock() = hints.clone();
        if self.fails {
            return Err(EngineError("the model's files are damaged".to_string()));
        }
        // As the model can't take more than it was made for.
        if piece.len() > self.longest.0 {
            return Err(EngineError(format!("a piece of {} samples is longer than {}", piece.len(), self.longest.0)));
        }
        let mut text = String::new();
        let mut sentence_begins = true;
        for word in self.words(piece, hints) {
            if !text.is_empty() {
                text.push(' ');
            }
            let mut chars = word.text.chars();
            if sentence_begins && let Some(first) = chars.next() {
                text.extend(first.to_uppercase());
            }
            text.push_str(chars.as_str());
            if word.ends_a_sentence {
                text.push('.');
            }
            sentence_begins = word.ends_a_sentence;
        }
        if self.leaves_off_the_last_full_stop {
            text.truncate(text.trim_end_matches('.').len());
        } else if !text.is_empty() && !text.ends_with('.') {
            text.push('.');
        }
        Ok(text)
    }
}

/// A recording the fake hears, kept whole, since tests' recordings are short.
struct FakeListening<'e> {
    engine: &'e FakeEngine,
    hints: Hints,
    audio: Vec<f32>,
    /// How many of its words it has given.
    told: usize,
}

impl FakeListening<'_> {
    /// The words ready to give, or all of them once `everything` has been said, each after the full stop that ends
    /// the sentence before it, if one does, and once everything has been said, the last sentence's too.
    fn heard(&mut self, everything: bool) -> Vec<Heard> {
        let now = self.audio.len();
        let words = self.engine.words(&self.audio, &self.hints);
        let held_back = self.engine.held_back;
        let mut heard = Vec::new();
        for (i, word) in words.iter().enumerate().skip(self.told) {
            if !(everything || word.at.0 + held_back.0 <= now) {
                break;
            }
            if i > 0 && words[i - 1].ends_a_sentence {
                heard.push(Heard { text: ".".to_string(), at: word.at });
            }
            heard.push(Heard { text: format!(" {}", word.text), at: word.at });
            self.told += 1;
        }
        if everything && let Some(last) = words.last().filter(|word| word.ends_a_sentence) {
            heard.push(Heard { text: ".".to_string(), at: last.at + WORD });
        }
        heard
    }
}

impl Listening for FakeListening<'_> {
    fn hear(&mut self, audio: &[f32]) -> Result<Vec<Heard>, EngineError> {
        self.audio.extend_from_slice(audio);
        Ok(self.heard(false))
    }

    fn finish(&mut self) -> Result<Vec<Heard>, EngineError> {
        Ok(self.heard(true))
    }

    fn heard_through(&self) -> Samples {
        Samples(self.audio.len().saturating_sub(self.engine.held_back.0))
    }
}

/// Where each burst of a word begins and ends in `audio`, one still sounding at its end included.
fn bursts(audio: &[f32]) -> Vec<(usize, usize)> {
    let mut bursts = Vec::new();
    let mut started = None;
    for (i, step) in audio.chunks(STEP).enumerate() {
        match (started, level_db(step) > BURST_DB) {
            (None, true) => started = Some(i * STEP),
            (Some(start), false) => {
                bursts.push((start, i * STEP));
                started = None;
            }
            (None, false) | (Some(_), true) => {}
        }
    }
    bursts.extend(started.map(|start| (start, audio.len())));
    bursts
}

/// The pitch of a burst, from how often it crosses zero going up.
fn pitch(burst: &[f32]) -> f32 {
    let ups = burst.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    small(ups) * small(Samples::SECOND.0) / small(burst.len())
}

/// A word: a tone at `hz` and `loudness`, fading in and out over 10 ms so it doesn't click.
fn tone(hz: f32, loudness: f32) -> Vec<f32> {
    let fade = small(STEP);
    (0..WORD.0)
        .map(|i| {
            let edge = (small(i).min(small(WORD.0 - i)) / fade).min(1.0);
            loudness * edge * (std::f32::consts::TAU * hz * small(i) / small(Samples::SECOND.0)).sin()
        })
        .collect()
}

/// A quiet room's noise, the same every time.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (small(usize::try_from(self.0 >> 16).unwrap_or(0)) / 32_768.0 - 1.0) * 0.001
    }
}

#[expect(clippy::cast_precision_loss, reason = "counts in a test's short recording, which an f32 holds exactly")]
fn small(n: usize) -> f32 {
    n as f32
}

/// The whole number nearest `x`, which is never far from one. Below zero is the first.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a word's place in a short script")]
fn nearest(x: f32) -> usize {
    x.round().max(0.0) as usize
}
