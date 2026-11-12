//! What the pointer does to a selection in a transcript's list: a press starts one, a drag moves its far end, and a
//! pointer held past the list's top or bottom scrolls the list on, faster the further past it is.

use std::time::{Duration, Instant};

use gpui_kit::base::GlobalState;
use gpui_kit::{
    Bounds, DispatchPhase, Hitbox, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Window, px,
};

use super::Selection;

/// How fast the list scrolls under a pointer held past its edge, in points a second for each point past it.
const PAST_THE_EDGE: f32 = 30.;
/// The slowest and fastest it scrolls, in points a second.
const SLOWEST: f32 = 240.;
const FASTEST: f32 = 4800.;
/// The longest a frame is taken to have been, so a stall doesn't throw the list a long way.
const LONGEST_FRAME: Duration = Duration::from_millis(50);

impl Selection {
    /// Listens for a press in the list at `hitbox`, in `bounds`, to select the text it's on or let go of what's
    /// selected, and while one's held, for where the pointer goes and for the button coming up.
    ///
    /// A press is heard after what's drawn over the list, such as the scrollbar, and what's in its rows, so one that
    /// takes a press keeps it, and before the kit's selection, which lets go of what it had selected as a press goes
    /// down. One on a text has the kit select nothing from there, so the two are never both selecting. Where the
    /// pointer goes and the button coming up are heard before anything else, so a drag goes on over whatever it's on.
    pub(crate) fn hear_the_pointer(&self, hitbox: &Hitbox, bounds: Bounds<Pixels>, window: &mut Window) {
        let (pressed, hitbox) = (self.clone(), hitbox.clone());
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || event.button != MouseButton::Left || !hitbox.is_hovered(window) {
                return;
            }
            if pressed.press(event.position, event.click_count, event.modifiers.shift) {
                window.focus(&pressed.focus, cx);
                GlobalState::suppress_text_selection(cx);
            }
            window.refresh();
        });
        let dragged = self.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _| {
            if phase != DispatchPhase::Capture || !dragged.is_dragging() {
                return;
            }
            if event.pressed_button != Some(MouseButton::Left) {
                dragged.release();
                return;
            }
            dragged.drag_to(event.position, bounds);
            // Past the edge, the list moves on its own from the next frame, which asks for the one after.
            window.refresh();
        });
        let released = self.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
            if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                released.release();
            }
        });
    }

    /// How far to scroll a list in `bounds` down, at `now`, for a pointer held past its top or bottom, if it is.
    /// Up is less than nothing. It's nothing the first frame the pointer's past it, which only starts the clock.
    pub(crate) fn scroll_past_edge(&self, bounds: Bounds<Pixels>, now: Instant) -> Option<Pixels> {
        let mut selecting = self.inner.borrow_mut();
        let drag = selecting.drag.as_mut()?;
        let past = if drag.pointer.y < bounds.top() {
            drag.pointer.y - bounds.top()
        } else if drag.pointer.y > bounds.bottom() {
            drag.pointer.y - bounds.bottom()
        } else {
            drag.moved = None;
            return None;
        };
        let since = drag.moved.replace(now).map_or(Duration::ZERO, |then| now.saturating_duration_since(then));
        let speed = (f32::from(past.abs()) * PAST_THE_EDGE).clamp(SLOWEST, FASTEST);
        Some(px(past.signum() * speed * since.min(LONGEST_FRAME).as_secs_f32()))
    }
}
