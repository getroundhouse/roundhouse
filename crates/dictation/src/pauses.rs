//! Cutting a recording at its pauses, into pieces, which are heard whole a sentence at a time for their final text.
//!
//! Each 30 ms of audio is speech when it's well above the room's noise, whose level is the quietest the recording has
//! been lately. A piece ends a little into a pause that follows speech, so pieces meet with no audio between them or
//! in both. One that would be longer than the engine takes is cut sooner, at the latest of its quiet moments. Audio
//! with no speech is cut off in pieces of its own as it comes, so a piece doesn't start with a long silence.

use crate::Samples;

/// How much audio each level is taken over.
const FRAME: usize = Samples::from_millis(30).0;

/// How long a quiet has to last, after speech, to end a piece: longer than the gaps between words and sentences of
/// someone thinking as they speak, shorter than someone stopping to think.
const PAUSE_FRAMES: usize = 27;

/// How far into a pause a piece ends, so its last word's tail is in it.
const INTO_PAUSE_FRAMES: usize = 10;

/// How long a piece with no speech can grow before it's cut off, which a piece that's begun keeps.
const SILENCE_FRAMES: usize = 67;

/// How far back from where it would have to end, a piece that's too long is cut, or nearly all of it, for an engine
/// that takes pieces shorter than this.
const SEARCH_FRAMES: usize = 100;

/// How much louder than the quietest a moment can be and still count as quiet, so a piece that's too long is cut at
/// the latest of its quiet moments, which leaves the next piece the least, rather than at the very quietest.
const NEARLY_QUIETEST_DB: f32 = 3.0;

/// How much speech a piece needs to have speech in it, so a click or a knock isn't heard as some.
const LEAST_SPEECH_FRAMES: usize = 3;

/// How far above the room's noise speech is.
const ABOVE_THE_ROOM_DB: f32 = 12.0;

/// Nothing quieter is speech, however quiet the room.
const QUIETEST_SPEECH_DB: f32 = -55.0;

/// The room's noise until the recording says, a little above a quiet room's, so speech from the first moment counts.
const FIRST_ROOM_DB: f32 = -50.0;

/// The room is taken to be no quieter than this, which is below any microphone's own noise.
const QUIETEST_ROOM_DB: f32 = -70.0;

/// How fast the room's level rises after it was quieter, a frame at a time, so speech, with its gaps, doesn't raise it
/// far.
const ROOM_RISE_DB: f32 = 0.15;

/// Where a piece ends, and whether it had speech in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cut {
    pub(crate) at: Samples,
    pub(crate) speech: bool,
}

/// What's heard of the piece that hasn't ended.
pub(crate) struct Pauses {
    longest_frames: usize,
    /// How far back from `longest_frames` a piece that's too long is cut.
    search_frames: usize,
    room_db: f32,
    /// Audio that isn't a whole frame yet.
    partial: Vec<f32>,
    /// Where the piece begins.
    start: Samples,
    /// Each frame of it so far.
    frames: Vec<Frame>,
}

/// 30 ms of a piece.
#[derive(Clone, Copy, Debug)]
struct Frame {
    level_db: f32,
    speech: bool,
}

impl Pauses {
    /// For pieces no longer than `longest`, or than one frame, if it's shorter.
    pub(crate) fn new(longest: Samples) -> Self {
        let longest_frames = (longest.0 / FRAME).max(1);
        Self {
            longest_frames,
            // Leaves at least a frame in the piece that's cut.
            search_frames: SEARCH_FRAMES.min(longest_frames - 1),
            room_db: FIRST_ROOM_DB,
            partial: Vec::new(),
            start: Samples(0),
            frames: Vec::new(),
        }
    }

    /// Takes the next of the recording's audio, and gives where any piece ends in it.
    pub(crate) fn hear(&mut self, audio: &[f32]) -> Vec<Cut> {
        self.partial.extend_from_slice(audio);
        let whole = self.partial.len() / FRAME * FRAME;
        let frames: Vec<f32> = self.partial.drain(..whole).collect();
        frames.chunks(FRAME).filter_map(|frame| self.frame(level_db(frame))).collect()
    }

    /// Whether the piece that hasn't ended has speech in it so far.
    pub(crate) fn speech(&self) -> bool {
        has_speech(&self.frames)
    }

    fn frame(&mut self, level_db: f32) -> Option<Cut> {
        let speech = level_db >= (self.room_db + ABOVE_THE_ROOM_DB).max(QUIETEST_SPEECH_DB);
        self.room_db = if level_db < self.room_db {
            level_db.max(QUIETEST_ROOM_DB)
        } else {
            (self.room_db + ROOM_RISE_DB).min(level_db)
        };
        self.frames.push(Frame { level_db, speech });
        let quiet = self.frames.iter().rev().take_while(|frame| !frame.speech).count();
        let cut = if self.speech() {
            (quiet >= PAUSE_FRAMES).then(|| self.frames.len() - quiet + INTO_PAUSE_FRAMES)
        } else {
            (self.frames.len() >= SILENCE_FRAMES).then(|| self.frames.len() - INTO_PAUSE_FRAMES)
        };
        let cut = cut.or_else(|| (self.frames.len() >= self.longest_frames).then(|| self.latest_quiet()))?;
        Some(self.cut_at(cut))
    }

    /// The latest frame of the last `search_frames` that's nearly as quiet as the quietest of them, to cut a piece
    /// that's too long.
    fn latest_quiet(&self) -> usize {
        let from = self.frames.len() - self.search_frames;
        let last = &self.frames[from..];
        let quietest = last.iter().map(|frame| frame.level_db).fold(f32::INFINITY, f32::min);
        from + last.iter().rposition(|frame| frame.level_db <= quietest + NEARLY_QUIETEST_DB).unwrap_or(0)
    }

    /// Ends the piece before its frame `frame`, which begins the next.
    fn cut_at(&mut self, frame: usize) -> Cut {
        let speech = has_speech(&self.frames[..frame]);
        self.frames.drain(..frame);
        self.start = self.start + Samples(frame * FRAME);
        Cut { at: self.start, speech }
    }
}

fn has_speech(frames: &[Frame]) -> bool {
    frames.iter().filter(|frame| frame.speech).count() >= LEAST_SPEECH_FRAMES
}

/// The level of `audio`, in dB from a full-scale sine's.
pub(crate) fn level_db(audio: &[f32]) -> f32 {
    let power = audio.iter().map(|x| x * x).sum::<f32>() / count(audio.len()).max(1.0);
    10.0 * (power + 1e-12).log10() + 3.0
}

#[expect(clippy::cast_precision_loss, reason = "a frame's samples, far fewer than an f32 counts exactly")]
fn count(samples: usize) -> f32 {
    samples as f32
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::{Cut, FRAME, Pauses};
    use crate::Samples;
    use crate::fake::FakeEngine;

    const SCRIPT: &[&str] = &["one", "two", "three", "four", "five", "six"];

    fn cuts(audio: &[f32], longest: Samples) -> Vec<Cut> {
        let mut pauses = Pauses::new(longest);
        // In buffers of 10 ms, as the microphone gives them.
        audio.chunks(160).flat_map(|chunk| pauses.hear(chunk)).collect()
    }

    fn seconds(at: Samples) -> f32 {
        f32::from(u16::try_from(at.0 / 16).unwrap()) / 1000.0
    }

    #[test]
    fn a_piece_ends_a_little_into_each_pause_after_speech_and_not_between_words() {
        let engine = FakeEngine::new(SCRIPT);
        // Words of 0.3 s with 0.12 s between them, and 1.2 s for each `/`.
        let audio = engine.saying("one two three / four five / six");

        let cuts = cuts(&audio, Samples::from_secs(15));

        let at: Vec<f32> = cuts.iter().map(|cut| seconds(cut.at)).collect();
        assert_eq!(cuts.len(), 2, "{at:?}");
        assert!(cuts.iter().all(|cut| cut.speech));
        // Three words end at 1.14 s, and the pause runs to 2.34 s; then two words end at 3.18 s.
        assert!((1.3..1.6).contains(&at[0]), "{at:?}");
        assert!((3.3..3.6).contains(&at[1]), "{at:?}");
    }

    #[test]
    fn silence_is_cut_off_as_it_comes_so_a_piece_starts_near_its_speech() {
        let engine = FakeEngine::new(SCRIPT);
        let audio = engine.saying("/ / / one two");

        let cuts = cuts(&audio, Samples::from_secs(15));

        assert!(!cuts.is_empty());
        assert!(cuts.iter().all(|cut| !cut.speech), "{cuts:?}");
        let last = seconds(cuts.last().unwrap().at);
        // The words start at 3.6 s, and what's cut off ends no more than two seconds before.
        assert!((1.6..3.6).contains(&last), "{last}");
    }

    #[test]
    fn a_piece_that_would_be_too_long_is_cut_at_a_gap_between_words() {
        let engine = FakeEngine::new(SCRIPT);
        let audio = engine.saying("one two three four five six one two three four five six");
        let longest = Samples::from_secs(4);

        let cuts = cuts(&audio, longest);

        assert!(!cuts.is_empty());
        let mut start = Samples(0);
        for cut in &cuts {
            assert!(cut.at.0 - start.0 <= longest.0, "{cuts:?}");
            // At the latest gap, which is within a word's turn of the limit, so pieces are as long as they can be.
            assert!(cut.at.0 - start.0 >= longest.0 - Samples::from_millis(450).0, "{cuts:?}");
            // Between words, where it's quiet: a word is 0.3 s, and then 0.12 s of quiet.
            let into_word = Samples(cut.at.0 % Samples::from_millis(420).0);
            assert!(into_word >= Samples::from_millis(290), "cut {into_word:?} into a word's turn");
            start = cut.at;
        }
    }

    /// Audio made of `parts`, each a tone or a hiss at a level, or silence, for a while.
    fn made_of(parts: &[(u8, f32, usize)]) -> Vec<f32> {
        let mut audio = Vec::new();
        let mut hiss = 0x2545_f491_u32;
        for &(kind, level_db, millis) in parts {
            let amplitude = 10f32.powf(level_db / 20.0);
            let tone = FakeEngine::new(&["a"]).saying("a");
            for i in 0..Samples::from_millis(millis).0 {
                hiss ^= hiss << 13;
                hiss ^= hiss >> 17;
                hiss ^= hiss << 5;
                audio.push(match kind {
                    0 => 0.0,
                    1 => amplitude * tone[i % tone.len()],
                    _ => amplitude * (f32::from(u16::try_from(hiss >> 16).unwrap()) / 32768.0 - 1.0),
                });
            }
        }
        audio
    }

    proptest! {
        #![proptest_config(Config {
            rng_seed: RngSeed::Fixed(1),
            failure_persistence: None,
            cases: 64,
            ..Config::default()
        })]

        #[test]
        fn pieces_follow_each_other_each_a_frame_or_more_and_none_too_long(
            parts in prop::collection::vec((0u8..3, -70f32..0.0, 1usize..3000), 0..12),
            buffer in 1usize..2000,
            longest in FRAME..Samples::from_secs(20).0,
        ) {
            let audio = made_of(&parts);
            let longest = Samples(longest);
            let mut pauses = Pauses::new(longest);

            let cuts: Vec<Cut> = audio.chunks(buffer).flat_map(|chunk| pauses.hear(chunk)).collect();

            let mut start = 0;
            for cut in &cuts {
                prop_assert!(cut.at.0 >= start + FRAME, "{cuts:?}");
                prop_assert!(cut.at.0 - start <= longest.0, "{cuts:?}");
                start = cut.at.0;
            }
            prop_assert!(audio.len() - start <= longest.0, "what's left once it's stopped is no longer either");
            let mut whole = Pauses::new(longest);
            prop_assert_eq!(&cuts, &whole.hear(&audio), "however the audio comes");
        }
    }

    #[test]
    fn steady_noise_well_above_a_quiet_room_is_not_speech_once_heard_for_a_moment() {
        // A room 20 dB louder than a quiet one's start, such as a fan right by the microphone. While the room's level
        // rises to it, it's speech, and once it has, the pieces cut off have none.
        let noise: Vec<f32> = (0..Samples::from_secs(12).0).map(|i| if i % 2 == 0 { 0.03 } else { -0.03 }).collect();

        let cuts = cuts(&noise, Samples::from_secs(15));

        assert!(!cuts.is_empty());
        assert!(!cuts.last().unwrap().speech, "{cuts:?}");
    }
}
