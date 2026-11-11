//! The wheel as the transcript hears it: one gesture at a time, which way each goes, and what it goes to.
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
//! On a trackpad, a gesture goes the way its first movement went, down or sideways, until it ends, so a swipe down that
//! wobbles doesn't drift a block sideways, and a swipe sideways doesn't scroll one down. The transcript's list decides
//! that for itself, as GPUI does: by the gaps between events, and letting a gesture turn once it turns far enough.
//!
//! It's heard once, before anything in the transcript acts on it, since an area only hears the wheel while it's drawn:
//! one that scrolls into view mid-gesture never heard it start.

use std::time::{Duration, Instant};

use gpui_kit::{
    Axis, DispatchPhase, ElementId, Global, IntoElement, Pixels, Point, ScrollEvent, Styled as _, TouchPhase, Window,
    canvas, point, px,
};

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
    /// Which way the gesture goes, once it has moved.
    axis: Option<Axis>,
    /// How far the event being heard moves, along the way the gesture goes.
    pub(super) delta: Point<Pixels>,
    /// Whether the event being heard is part of a fling.
    pub(super) flinging: bool,
    pub(super) owner: Owner,
}

impl Global for Wheel {}

impl Wheel {
    /// Notes the event at `phase` of a gesture, which moves by `delta`, before any area acts. A trackpad's movement is
    /// `precise`, and a mouse wheel's notches aren't.
    fn hear(&mut self, phase: TouchPhase, delta: Point<Pixels>, precise: bool, now: Instant) {
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
            self.axis = None;
        } else if self.moved && self.owner == Owner::Undecided {
            // No area took the gesture's first movement, so the transcript did.
            self.owner = Owner::Transcript;
        }
        self.flinging = self.fingers == Fingers::Lifted && phase == TouchPhase::Moved;
        self.delta = self.along_the_way(delta, precise);
        self.moved |= self.delta != Point::default();
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.fingers = Fingers::Lifted;
        }
        self.last_at = Some(now);
    }

    /// `delta` along the way the gesture goes, which its first movement decides. A mouse wheel's notches each go one
    /// way, so they're taken as they come.
    fn along_the_way(&mut self, delta: Point<Pixels>, precise: bool) -> Point<Pixels> {
        if !precise || delta == Point::default() {
            return delta;
        }
        let first = if delta.x.abs() > delta.y.abs() { Axis::Horizontal } else { Axis::Vertical };
        match *self.axis.get_or_insert(first) {
            Axis::Vertical => point(px(0.), delta.y),
            Axis::Horizontal => point(delta.x, px(0.)),
        }
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
                cx.default_global::<Wheel>().hear(event.touch_phase, delta, event.delta.precise(), now);
            });
        },
    )
    .absolute()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui_kit::TouchPhase::{self, Ended, Moved, Started};
    use gpui_kit::{ElementId, Pixels, Point, point, px};

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

        /// Whether each of `phases`, each moving down, was heard as part of a fling.
        fn flinging(&mut self, phases: &[TouchPhase]) -> Vec<bool> {
            phases
                .iter()
                .map(|phase| {
                    self.heard(*phase, point(px(0.), px(-10.)), true);
                    self.wheel.flinging
                })
                .collect()
        }

        /// The fingers touching or lifting without moving, as a trackpad says they have.
        fn still(&mut self, phase: TouchPhase) {
            self.heard(phase, Point::default(), true);
        }

        /// How far the fingers moving by `delta` move along the way the gesture goes.
        fn swiped(&mut self, delta: Point<Pixels>) -> Point<Pixels> {
            self.heard(Moved, delta, true);
            self.wheel.delta
        }

        fn heard(&mut self, phase: TouchPhase, delta: Point<Pixels>, precise: bool) {
            self.now += Duration::from_millis(16);
            self.wheel.hear(phase, delta, precise, self.now);
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
    fn a_gesture_goes_the_way_it_first_moved_until_it_ends() {
        let mut heard = Heard::new();
        heard.still(Started);

        assert_eq!(heard.swiped(point(px(-2.), px(-10.))), point(px(0.), px(-10.)), "down, as it mostly went");
        assert_eq!(heard.swiped(point(px(30.), px(-4.))), point(px(0.), px(-4.)), "and not sideways, however far");

        heard.still(Ended);
        assert_eq!(heard.swiped(point(px(20.), px(-3.))), point(px(0.), px(-3.)), "through the fling");

        heard.still(Started);
        assert_eq!(
            heard.swiped(point(px(-12.), px(-5.))),
            point(px(-12.), px(0.)),
            "and a new gesture can go sideways"
        );
        assert!(heard.wheel.moved, "which is moving, as going down is");
    }

    #[test]
    fn a_mouse_wheel_s_notches_go_as_they_come() {
        let mut heard = Heard::new();
        heard.heard(Moved, point(px(0.), px(-40.)), false);

        heard.heard(Moved, point(px(-40.), px(0.)), false);

        assert_eq!(heard.wheel.delta, point(px(-40.), px(0.)), "a notch sideways, in the same spin as one down");
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
