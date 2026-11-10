//! The wheel as the transcript hears it: one gesture at a time, and what each gesture goes to.
//!
//! A gesture goes to what its first movement scrolls: an area under the pointer that can move that way, or else the
//! transcript. It stays there, even as other areas pass under the pointer, so a gesture that starts on the transcript
//! carries on over the blocks it meets. An area at its end hands the rest of the gesture to the transcript, except a
//! fling, which stops there instead of carrying the transcript on at its speed.
//!
//! On a trackpad, a gesture runs from the fingers touching to the end of the fling they leave when they lift. GPUI
//! says when they touch and lift, and calls the fling a plain movement. A mouse wheel never touches or lifts, so a
//! gesture is a run of its notches, which a pause ends.
//!
//! It's heard once, before anything in the transcript acts on it, since an area only hears the wheel while it's drawn:
//! one that scrolls into view mid-gesture never heard it start.

use std::time::{Duration, Instant};

use gpui_kit::{DispatchPhase, ElementId, Global, IntoElement, ScrollEvent, Styled as _, TouchPhase, Window, canvas};

/// A fling arrives once a frame until it stops, so a longer silence means it's over. The kit's own scroll areas wait
/// as long, for the same reason.
const FLING_GAP: Duration = Duration::from_millis(250);
/// A wheel spun in one go sends its notches well within this, so a longer pause starts a new gesture. Long enough
/// that a block rolling under a spinning wheel doesn't catch it, and short enough that a new spin over the block
/// doesn't seem to wait.
const SPIN_GAP: Duration = Duration::from_millis(500);

/// What a gesture goes to.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) enum Owner {
    /// Nothing has moved yet.
    #[default]
    Undecided,
    /// The area with this ID, which the gesture started over.
    Area(ElementId),
    /// The transcript, which the gesture started on, or which an area at its end handed it to.
    Transcript,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Fingers {
    /// Not on a trackpad, as with a mouse wheel.
    #[default]
    Away,
    Down,
    /// Lifted, so what comes now is the fling they left.
    Lifted,
}

#[derive(Default)]
pub(super) struct Wheel {
    last_at: Option<Instant>,
    fingers: Fingers,
    /// Whether the gesture has moved yet.
    moved: bool,
    /// Whether the event being heard is part of a fling.
    pub(super) flinging: bool,
    pub(super) owner: Owner,
}

impl Global for Wheel {}

impl Wheel {
    /// Notes the event at `phase` of a gesture, which `moves` the way an area scrolls or not, before any area acts.
    fn hear(&mut self, phase: TouchPhase, moves: bool, now: Instant) {
        let quiet = |gap| self.last_at.is_none_or(|at| now.saturating_duration_since(at) >= gap);
        let fresh = phase == TouchPhase::Started
            || match self.fingers {
                Fingers::Down => false,
                Fingers::Lifted => quiet(FLING_GAP),
                Fingers::Away => quiet(SPIN_GAP),
            };
        if fresh {
            self.fingers = if phase == TouchPhase::Started { Fingers::Down } else { Fingers::Away };
            self.owner = Owner::Undecided;
            self.moved = false;
        } else if self.moved && self.owner == Owner::Undecided {
            // No area took the gesture's first movement, so the transcript did.
            self.owner = Owner::Transcript;
        }
        self.flinging = self.fingers == Fingers::Lifted && phase == TouchPhase::Moved;
        self.moved |= moves;
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.fingers = Fingers::Lifted;
        }
        self.last_at = Some(now);
    }
}

/// Hears each wheel event in the window before anything drawn after it does. The transcript draws it before its rows.
pub(crate) fn hears_the_wheel() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, (), window: &mut Window, _| {
            window.on_mouse_event(|event: &ScrollEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let delta = event.delta.pixel_delta(window.line_height());
                let now = cx.background_executor().now();
                cx.default_global::<Wheel>().hear(event.touch_phase, delta.y.abs() > delta.x.abs(), now);
            });
        },
    )
    .absolute()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui_kit::ElementId;
    use gpui_kit::TouchPhase::{self, Ended, Moved, Started};

    use super::{FLING_GAP, Owner, SPIN_GAP, Wheel};

    /// A wheel that hears each event a frame after the last.
    struct Heard {
        wheel: Wheel,
        now: Instant,
    }

    impl Heard {
        fn new() -> Self {
            Self { wheel: Wheel::default(), now: Instant::now() }
        }

        /// Whether each of `phases`, each moving, was heard as part of a fling.
        fn flinging(&mut self, phases: &[TouchPhase]) -> Vec<bool> {
            phases
                .iter()
                .map(|phase| {
                    self.now += Duration::from_millis(16);
                    self.wheel.hear(*phase, true, self.now);
                    self.wheel.flinging
                })
                .collect()
        }

        /// The fingers touching or lifting without moving, as a trackpad says they have.
        fn still(&mut self, phase: TouchPhase) {
            self.now += Duration::from_millis(16);
            self.wheel.hear(phase, false, self.now);
        }
    }

    fn area() -> Owner {
        Owner::Area(ElementId::Name("output".into()))
    }

    #[test]
    fn what_comes_after_the_fingers_lift_is_a_fling() {
        let mut heard = Heard::new();

        assert_eq!(heard.flinging(&[Started, Moved, Ended, Moved, Moved]), [false, false, false, true, true]);
    }

    #[test]
    fn a_mouse_wheel_never_flings() {
        let mut heard = Heard::new();

        assert_eq!(heard.flinging(&[Moved, Moved, Moved]), [false, false, false]);
    }

    #[test]
    fn a_gesture_no_area_takes_at_first_goes_to_the_transcript() {
        let mut heard = Heard::new();
        heard.still(Started);
        heard.flinging(&[Moved]);
        assert_eq!(heard.wheel.owner, Owner::Undecided, "until its first movement has gone by");

        heard.flinging(&[Moved]);

        assert_eq!(heard.wheel.owner, Owner::Transcript);
    }

    #[test]
    fn a_gesture_keeps_what_it_went_to_until_it_ends() {
        let mut heard = Heard::new();
        heard.still(Started);
        heard.wheel.owner = area();

        heard.flinging(&[Moved]);
        heard.now += SPIN_GAP;
        heard.flinging(&[Moved]);
        heard.still(Ended);
        heard.flinging(&[Moved]);

        assert_eq!(heard.wheel.owner, area(), "through a pause with the fingers down, the lift and the fling");
    }

    #[test]
    fn touching_again_or_a_pause_starts_a_new_gesture() {
        let mut heard = Heard::new();
        heard.still(Started);
        heard.wheel.owner = area();
        heard.still(Started);
        assert_eq!(heard.wheel.owner, Owner::Undecided, "touching again");

        heard.wheel.owner = area();
        heard.still(Ended);
        heard.now += FLING_GAP;
        heard.flinging(&[Moved]);
        assert_eq!(heard.wheel.owner, Owner::Undecided, "a fling that went quiet");

        heard.wheel.owner = area();
        heard.now += SPIN_GAP;
        heard.flinging(&[Moved]);
        assert_eq!(heard.wheel.owner, Owner::Undecided, "a wheel left still");
    }
}
