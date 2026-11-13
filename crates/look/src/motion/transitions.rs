use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::base::motion::Transition;
use gpui_kit::{App, Window};
use std::time::{Duration, Instant};

/// How long a panel takes to slide into place or away, such as a sidebar. Long enough to see where it went, and short
/// enough not to wait for.
pub const PANEL: Duration = Duration::from_millis(200);
/// How long something small takes to fade in or out, such as a button that comes and goes.
pub const FADE: Duration = Duration::from_millis(200);
/// How long a scroll takes to get where it's sent, such as a chat sent back down to its latest row. Longer than a
/// slide, since a scroll goes further, and rows passed more quickly can't be followed by eye.
pub const SCROLL: Duration = Duration::from_millis(300);

/// For something small fading in or out. It slows as it arrives, which is the kit's curve for a transition.
pub fn fade() -> Transition {
    Transition::new(FADE)
}

/// A scroll on its way over [`SCROLL`] to a place that can move meanwhile, such as a chat's latest row as more of the
/// chat comes in. It slows toward the end, as a slide does. It says what share of the way is still to go rather than
/// where to be, so on each frame whatever scrolls moves by all but that share of what it finds is left, and gets
/// wherever the place has moved to. What comes in early on is scrolled to with the rest. What comes in near the end is
/// taken in almost at once, as it is by a chat already at its end.
#[derive(Debug)]
pub struct Scrolling {
    /// When it set off, by the app's clock.
    pub(super) since: Instant,
    /// The share of its way it had still to go when it was last moved on.
    pub(super) left: f32,
}

impl Scrolling {
    /// One setting off now. Under Reduce Motion, it's there the first time it's moved on.
    pub fn set_off(cx: &App) -> Self {
        Self { since: cx.background_executor().now(), left: 1. }
    }

    /// One that set off at `since`, for a test that keeps a clock of its own.
    #[cfg(any(test, feature = "testing"))]
    pub fn since(since: Instant) -> Self {
        Self { since, left: 1. }
    }

    /// Moves it on to now, and says what share of what was left the time before is still to go: 1 if no time has
    /// passed, and 0 once it's there, as it is at once under Reduce Motion, even if that was turned on partway. Asks for
    /// the next frame while it's on its way.
    pub fn move_on(&mut self, window: &mut Window, cx: &App) -> f32 {
        let share = if cx.reduce_motion() { self.arrive() } else { self.move_on_to(cx.background_executor().now()) };
        if share > 0. {
            window.request_animation_frame();
        }
        share
    }

    /// Moves it on to `now`, by the app's clock, and says what share of what was left is still to go.
    pub(super) fn move_on_to(&mut self, now: Instant) -> f32 {
        if self.left <= 0. {
            return 0.;
        }
        let left = 1. - eased(now.saturating_duration_since(self.since), SCROLL);
        let share = left / self.left;
        self.left = left;
        share
    }

    /// Puts it where it's going at once.
    pub(super) fn arrive(&mut self) -> f32 {
        self.left = 0.;
        0.
    }
}

/// How far along something that takes `over` to move is, `elapsed` after it set off, from 0 to 1, slowing as it
/// arrives.
fn eased(elapsed: Duration, over: Duration) -> f32 {
    ease_out_cubic((elapsed.as_secs_f32() / over.as_secs_f32()).min(1.))
}

/// A value on its way somewhere over [`PANEL`], slowing as it arrives, such as how far open a sidebar is. Whatever
/// owns the value keeps this, rather than a view, so one drawn for the first time partway finds it where it is. It
/// reads the clock each time it's asked, so its owner asks once as a frame starts and every view goes by that.
#[derive(Clone, Copy, Debug)]
pub struct Sliding {
    pub(super) from: f32,
    pub(super) to: f32,
    /// When it set off, by the app's clock. `None` for one that started where it is.
    pub(super) since: Option<Instant>,
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
        if self.moving(cx) {
            window.request_animation_frame();
        }
        self.value(cx)
    }

    /// Whether it's still on its way, which it is until its time is up. Once it isn't, it's where it was going.
    pub fn moving(&self, cx: &App) -> bool {
        self.elapsed(cx).is_some_and(|elapsed| elapsed < PANEL)
    }

    /// How long it has been on its way, or `None` for one that isn't moving at all.
    pub(super) fn elapsed(&self, cx: &App) -> Option<Duration> {
        let since = self.since.filter(|_| !cx.reduce_motion())?;
        Some(cx.background_executor().now().saturating_duration_since(since))
    }

    /// Where it is `elapsed` after setting off.
    pub(super) fn after(&self, elapsed: Duration) -> f32 {
        self.from + (self.to - self.from) * eased(elapsed, PANEL)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{PANEL, SCROLL, Scrolling, Sliding};

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

    #[test]
    fn a_scroll_has_less_of_what_was_left_to_go_each_time_slowing_as_it_arrives_and_none_once_its_time_is_up() {
        let from = Instant::now();
        let mut scrolling = Scrolling::since(from);

        assert!((scrolling.move_on_to(from) - 1.).abs() < f32::EPSILON, "nothing has gone as it sets off");
        let shares: Vec<f32> =
            (1..=9_u8).map(|tenth| scrolling.move_on_to(from + SCROLL / 10 * u32::from(tenth))).collect();
        let left: Vec<f32> = shares
            .iter()
            .scan(1., |left, share| {
                *left *= share;
                Some(*left)
            })
            .collect();
        assert!(shares.iter().all(|share| *share > 0. && *share < 1.), "{shares:?}");
        assert!(1. - left[0] > left[7] - left[8], "the first tenth covers more than the ninth: {left:?}");
        assert!((left[4] - 0.125).abs() < 0.001, "an eighth of the way is left halfway through: {left:?}");

        assert!(scrolling.move_on_to(from + SCROLL).abs() < f32::EPSILON, "it's there once its time is up");
        assert!(scrolling.move_on_to(from + SCROLL * 3).abs() < f32::EPSILON, "and stays there");
    }
}
