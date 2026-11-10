//! How long things take to move, so everything that moves the same way takes the same time, and the two ways a value
//! moves. One that a single view draws follows the kit's `transition`, with a policy from here. One that several
//! views lay out by is a [`Sliding`], kept where they can all read it. Both skip the motion under Reduce Motion, and
//! ask for frames only while they move. `docs/gpui.md` has the rules.

use std::time::{Duration, Instant};

use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::base::motion::Transition;
use gpui_kit::{App, Window};

/// How long a panel takes to slide into place or away, such as a sidebar. Long enough to see where it went, and short
/// enough not to wait for.
pub const PANEL: Duration = Duration::from_millis(200);
/// How long something small takes to fade in or out, such as a button that comes and goes.
pub const FADE: Duration = Duration::from_millis(200);

/// For something small fading in or out. It slows as it arrives, which is the kit's curve for a transition.
pub fn fade() -> Transition {
    Transition::new(FADE)
}

/// A value on its way somewhere over [`PANEL`], slowing as it arrives, such as how far open a sidebar is. Whatever
/// owns the value keeps this, rather than a view, so one drawn for the first time partway finds it where it is. It
/// reads the clock each time it's asked, so its owner asks once as a frame starts and every view goes by that.
#[derive(Clone, Copy, Debug)]
pub struct Sliding {
    from: f32,
    to: f32,
    /// When it set off, by the app's clock. `None` for one that started where it is.
    since: Option<Instant>,
}

impl Sliding {
    /// At rest at `value`.
    pub const fn at(value: f32) -> Self {
        Self { from: value, to: value, since: None }
    }

    /// Sends it to `to`, from wherever it has got to, so one turned back partway doesn't jump. Under Reduce Motion it's
    /// there at once, and stays there if motion is allowed again a moment later.
    pub fn go(&mut self, to: f32, cx: &App) {
        *self = if cx.reduce_motion() {
            Self::at(to)
        } else {
            Self { from: self.value(cx), to, since: Some(cx.background_executor().now()) }
        };
    }

    /// Where it is now. Under Reduce Motion that's where it's going.
    pub fn value(&self, cx: &App) -> f32 {
        match self.elapsed(cx) {
            Some(elapsed) => self.after(elapsed),
            None => self.to,
        }
    }

    /// Where it is now, for something drawn by it, which is drawn again on the next frame for as long as it moves.
    pub fn draw(&self, window: &mut Window, cx: &App) -> f32 {
        if self.elapsed(cx).is_some_and(|elapsed| elapsed < PANEL) {
            window.request_animation_frame();
        }
        self.value(cx)
    }

    /// How long it has been on its way, or `None` for one that isn't moving at all.
    fn elapsed(&self, cx: &App) -> Option<Duration> {
        let since = self.since.filter(|_| !cx.reduce_motion())?;
        Some(cx.background_executor().now().saturating_duration_since(since))
    }

    /// Where it is `elapsed` after setting off.
    fn after(&self, elapsed: Duration) -> f32 {
        let part = (elapsed.as_secs_f32() / PANEL.as_secs_f32()).min(1.);
        self.from + (self.to - self.from) * ease_out_cubic(part)
    }
}

#[cfg(test)]
mod tests {
    use super::{PANEL, Sliding};

    fn sliding(from: f32, to: f32) -> Sliding {
        Sliding { from, to, since: None }
    }

    #[test]
    fn a_slide_starts_where_it_was_and_ends_exactly_where_its_going() {
        let shut = sliding(1., 0.);

        assert!((shut.after(PANEL.mul_f32(0.)) - 1.).abs() < f32::EPSILON);
        assert!(shut.after(PANEL).abs() < f32::EPSILON);
        assert!(shut.after(PANEL * 3).abs() < f32::EPSILON, "and stays there");
    }

    #[test]
    fn a_slide_only_ever_moves_toward_where_its_going_and_slows_as_it_arrives() {
        let open = sliding(0.25, 1.);
        let at: Vec<f32> = (0..=10_u8).map(|tenth| open.after(PANEL.mul_f32(f32::from(tenth) / 10.))).collect();

        assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{at:?}");
        assert!(at[1] - at[0] > at[10] - at[9], "the first tenth covers more than the last: {at:?}");
    }
}
