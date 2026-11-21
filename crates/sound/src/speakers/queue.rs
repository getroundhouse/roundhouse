//! What's still to be played, which the device's thread takes from as it plays, and the marks that say how far it has
//! got. A sample counts as played once it's given to the device, a moment before it's heard.

use std::collections::VecDeque;

/// Audio waiting to be played, in one channel at the device's rate, and the marks given between it.
#[derive(Debug, Default)]
pub(super) struct Queue {
    samples: VecDeque<f32>,
    /// Each mark, after how many samples, counted from the first ever given, it's reached.
    marks: VecDeque<(u64, u64)>,
    /// How many samples have been given, counted from the first.
    given: u64,
    /// How many samples have been played, counted from the first.
    played: u64,
}

impl Queue {
    /// Plays `audio` after what's still to be played.
    pub(super) fn give(&mut self, audio: &[f32]) {
        self.samples.extend(audio);
        self.given += audio.len() as u64;
    }

    /// Puts `mark` after everything given so far, so it's reached once all of that has been played.
    pub(super) fn mark(&mut self, mark: u64) {
        self.marks.push_back((self.given, mark));
    }

    /// Drops what's still to be played, and its marks, which are never reached.
    pub(super) fn clear(&mut self) {
        self.samples.clear();
        self.marks.clear();
        self.played = self.given;
    }

    /// Fills `out`, frames of `channels` samples each, with the next sample in every channel of a frame, made into the
    /// device's format by `format`, and silence once there's nothing left. Then tells `reached` each mark everything
    /// before which has been played, in the order they were given.
    pub(super) fn fill<T: Copy>(
        &mut self,
        out: &mut [T],
        channels: usize,
        format: impl Fn(f32) -> T,
        reached: &mut impl FnMut(u64),
    ) {
        for frame in out.chunks_mut(channels.max(1)) {
            let sample = self.samples.pop_front();
            if sample.is_some() {
                self.played += 1;
            }
            frame.fill(format(sample.unwrap_or(0.0)));
        }
        while let Some(&(at, mark)) = self.marks.front()
            && at <= self.played
        {
            self.marks.pop_front();
            reached(mark);
        }
    }
}
