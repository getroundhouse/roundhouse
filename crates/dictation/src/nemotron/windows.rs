//! Where the live encoder's windows fall, and which of each window's steps are decoded.
//!
//! The live encoder takes 6.16 s of audio at a time: 70 steps of 80 ms that have been heard before, for context, then
//! 2 new ones, and 5 more after them, which it hears to make out the 2 but whose words are left for the next window,
//! since it hasn't heard what follows them. So a word is heard about 0.4 s after it ends. The first window takes the
//! 5 steps after its 2 at once. Once the recording ends, the windows take what's left, and the last one's steps after
//! its new ones are decoded too, since nothing follows them. Ported from `FluidAudio`'s `UnifiedStreamingWindower`.

use std::ops::Range;

use crate::Samples;

/// A count of the encoder's steps, or a place in a recording counted in them: each is 80 ms of audio, and what the
/// encoder makes out of it is decoded as one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Steps(pub(super) usize);

impl Steps {
    /// How much audio a step is.
    pub(super) const LENGTH: Samples = Samples::from_millis(80);

    /// Where the step starts in the recording, or how long the steps are.
    pub(super) const fn audio(self) -> Samples {
        Samples(self.0 * Self::LENGTH.0)
    }
}

/// The steps heard before, then new, then after, in each live window: the 70/2/5 the live encoder was exported for.
const BEFORE: Steps = Steps(70);
const NEW: Steps = Steps(2);
const AFTER: Steps = Steps(5);

/// How much audio a live window holds, which the encoder takes padded to it.
pub(super) const WINDOW: Samples = Steps(BEFORE.0 + NEW.0 + AFTER.0).audio();

/// Where the windows have got to in one recording.
#[derive(Debug, Default)]
pub(super) struct Windows {
    /// How much of the recording the windows have taken so far.
    taken: Samples,
    /// Where decoding has got to: every step before it has been decoded, or skipped, as `decode` says.
    decoded: Steps,
    /// The last window, after the recording ended, has been given.
    done: bool,
}

/// One window of the recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Window {
    /// Where it starts in the recording, on a step's start.
    pub(super) start: Steps,
    /// Where its audio ends, and its padding starts.
    pub(super) end: Samples,
    /// Whether it's the last of the recording, whose steps after its new ones are decoded too.
    last: bool,
}

impl Windows {
    /// The next window, once `heard` of the recording has been heard and more may come.
    pub(super) fn next(&mut self, heard: Samples) -> Option<Window> {
        let end = self.taken + self.taking().audio();
        (end <= heard && !self.done).then(|| self.take(end, false))
    }

    /// The next window, once the recording has ended after `heard`, until there are none.
    pub(super) fn next_at_the_end(&mut self, heard: Samples) -> Option<Window> {
        if self.done || heard == Samples(0) {
            return None;
        }
        let end = (self.taken + self.taking().audio()).min(heard);
        Some(self.take(end, end == heard))
    }

    /// The steps of `window` to decode, given that the encoder made out what it held up to `made_out`, a place in the
    /// recording. They follow those decoded before, and are counted as decoded.
    pub(super) fn decode(&mut self, window: Window, made_out: Steps) -> Range<Steps> {
        let held_back = if window.last { 0 } else { AFTER.0 };
        let to = Steps(made_out.0.saturating_sub(held_back));
        // Each window starts 70 steps before its new ones, and so well before the first step not yet decoded, unless the
        // encoder has made out less than its windows held for many windows running. Steps before the window can't be
        // decoded any more, so they're skipped.
        let from = self.decoded.max(window.start);
        if to <= from {
            return from..from;
        }
        self.decoded = to;
        from..to
    }

    /// How far into the recording every step has been decoded.
    pub(super) fn decoded_through(&self) -> Samples {
        self.decoded.audio()
    }

    /// Where the audio a window can still need starts, so what's before it can be let go of.
    pub(super) fn needed_from(&self) -> Samples {
        Samples(self.taken.0.saturating_sub(WINDOW.0))
    }

    /// How much more the next window takes: its new steps, and the first time the steps after them too.
    fn taking(&self) -> Steps {
        if self.taken == Samples(0) { Steps(NEW.0 + AFTER.0) } else { NEW }
    }

    fn take(&mut self, end: Samples, last: bool) -> Window {
        self.taken = end;
        self.done = last;
        // On a step's start, so its steps are the recording's.
        let start = Steps(end.0.saturating_sub(WINDOW.0).div_ceil(Steps::LENGTH.0));
        Window { start, end, last }
    }
}

impl Window {
    /// How much audio it holds, before its padding.
    pub(super) fn audio(self) -> Samples {
        self.end - self.start.audio()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::{Steps, WINDOW, Window, Windows};
    use crate::Samples;

    /// Where the encoder's steps of `window` end: one step for each 8 of the features' 10 ms frames, rounding up, as
    /// its three convolutions that each halve the frames do.
    fn made_out(window: Window) -> Steps {
        Steps(window.start.0 + (window.audio().0 / 160).div_ceil(8))
    }

    /// Every step decoded, in order, as the recording comes in `buffers` and then ends, and each window with where the
    /// audio it could need started when it was taken.
    fn decoded(buffers: &[usize]) -> (Vec<usize>, Vec<(Window, Samples)>) {
        let mut windows = Windows::default();
        let (mut heard, mut steps, mut all) = (Samples(0), Vec::new(), Vec::new());
        let mut run = |windows: &mut Windows, window: Window, needed_from: Samples, steps: &mut Vec<usize>| {
            let decoded = windows.decode(window, made_out(window));
            steps.extend(decoded.start.0..decoded.end.0);
            all.push((window, needed_from));
        };
        for buffer in buffers {
            heard = heard + Samples(*buffer);
            loop {
                let needed_from = windows.needed_from();
                let Some(window) = windows.next(heard) else { break };
                run(&mut windows, window, needed_from, &mut steps);
            }
        }
        loop {
            let needed_from = windows.needed_from();
            let Some(window) = windows.next_at_the_end(heard) else { break };
            run(&mut windows, window, needed_from, &mut steps);
        }
        (steps, all)
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        #[test]
        fn every_step_is_decoded_once_in_order_from_audio_still_kept_and_no_window_is_too_long(
            buffers in prop::collection::vec(0usize..20_000, 0..40),
        ) {
            let (steps, windows) = decoded(&buffers);

            let heard: usize = buffers.iter().sum();
            prop_assert_eq!(steps, (0..(heard / 160).div_ceil(8)).collect::<Vec<_>>());
            for (window, needed_from) in windows {
                prop_assert!(window.audio() <= WINDOW, "{:?}", window);
                prop_assert!(window.start.audio() >= needed_from, "{:?} starts before {:?}", window, needed_from);
            }
        }
    }

    #[test]
    fn a_step_is_decoded_once_the_five_after_it_have_been_heard() {
        let mut windows = Windows::default();

        let first = windows.next(Steps(7).audio()).unwrap();
        assert_eq!(windows.decode(first, Steps(7)), Steps(0)..Steps(2));
        assert_eq!(windows.next(Steps(8).audio()), None);
        let second = windows.next(Steps(9).audio()).unwrap();
        assert_eq!(windows.decode(second, Steps(9)), Steps(2)..Steps(4));
        assert_eq!(windows.decoded_through(), Steps(4).audio());
    }

    #[test]
    fn steps_the_windows_moved_past_before_they_were_made_out_are_skipped() {
        let mut windows = Windows::default();
        let mut heard = Steps(7);
        // The encoder makes out nothing of the first 40 windows, by which time they've moved past the first steps.
        for _ in 0..40 {
            let window = windows.next(heard.audio()).unwrap();
            assert!(windows.decode(window, window.start).is_empty());
            heard = Steps(heard.0 + 2);
        }

        let window = windows.next(heard.audio()).unwrap();

        assert_eq!(window.start, Steps(10), "{window:?}");
        assert_eq!(windows.decode(window, made_out(window)), Steps(10)..Steps(82), "from the window's first step");
    }

    #[test]
    fn a_recording_that_ends_where_a_window_did_has_that_window_again_for_its_last_steps() {
        let mut windows = Windows::default();
        let first = windows.next(Steps(7).audio()).unwrap();
        windows.decode(first, Steps(7));
        let second = windows.next(Steps(9).audio()).unwrap();
        windows.decode(second, Steps(9));

        let kept_from = windows.needed_from();
        let last = windows.next_at_the_end(Steps(9).audio()).unwrap();

        assert_eq!((last.start, last.end), (second.start, second.end), "the same audio");
        assert!(last.start.audio() >= kept_from, "the audio it needs was kept");
        assert_eq!(windows.decode(last, Steps(9)), Steps(4)..Steps(9));
        assert_eq!(windows.decoded_through(), Steps(9).audio());
        assert_eq!(windows.next_at_the_end(Steps(9).audio()), None);
    }
}
