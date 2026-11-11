//! An area in a row that scrolls what it holds under the wheel until it reaches the end the wheel is turning toward,
//! and then leaves the wheel to the transcript, so a long block can be read into the rest of the chat in one motion.
//! It only takes a gesture that starts over it, as [`wheel`] says, so one that starts on the transcript carries on
//! over it, and a trackpad's fling stops at its end instead of carrying the transcript on at its speed.
//!
//! GPUI's own `overflow_y_scroll` never gets the wheel here, and the kit's areas keep it even at their ends. The
//! transcript's list takes each wheel event in the capture phase, after everything inside it has had the chance and
//! before an area's own listener would see it. So this area acts in the capture phase too, after what it holds, and
//! stops the event only when it moved, or when it keeps its own fling from the transcript.
//!
//! An area of a file's lines scrolls sideways too, so a long line can be read to its end. It does that itself, as it
//! scrolls down, so a gesture moves only what it went to, and GPUI never moves it sideways. [`holding_wide`] lays it
//! out, and an edit's preview too, which keeps the wheel for itself and leaves GPUI to scroll it both ways.

use gpui_kit::base::{InteractiveElementExt as _, Scrollbar};
use gpui_kit::{
    AnyElement, App, DispatchPhase, Div, ElementId, HitboxBehavior, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, ScrollEvent, ScrollHandle, Stateful, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, canvas, div, point, px,
};

use self::wheel::{Owner, Wheel};

mod wheel;

pub(crate) use wheel::hears_the_wheel;

/// Holds `content`, at most `max_height` tall, scrolling it once it's taller, and once it's wider if it scrolls both
/// ways.
#[derive(IntoElement)]
pub(crate) struct ScrollArea {
    id: ElementId,
    max_height: Pixels,
    scrolls: Scrolls,
    content: AnyElement,
}

/// Which ways an area scrolls.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Scrolls {
    Down,
    /// Down, and sideways along its longest line.
    BothWays,
}

impl ScrollArea {
    /// `id` keeps the area's place from one frame to the next, so it's the area's alone.
    pub(crate) fn new(id: impl Into<ElementId>, max_height: Pixels, content: impl IntoElement) -> Self {
        Self { id: id.into(), max_height, scrolls: Scrolls::Down, content: content.into_any_element() }
    }

    pub(crate) fn max_height(mut self, max_height: Pixels) -> Self {
        self.max_height = max_height;
        self
    }

    /// Lays out what it holds as wide as its widest line, and scrolls sideways along it.
    pub(crate) fn both_ways(mut self) -> Self {
        self.scrolls = Scrolls::BothWays;
        self
    }
}

impl RenderOnce for ScrollArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handle = window.use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::new()).read(cx).clone();
        let area = div()
            .id((self.id.clone(), "area"))
            .max_h(self.max_height)
            .overflow_y_scroll()
            // GPUI otherwise turns a sideways swipe into a scroll down this area.
            .lock_scroll_axis()
            .track_scroll(&handle);
        let (area, bar) = match self.scrolls {
            Scrolls::Down => (area.child(self.content), Scrollbar::vertical(&handle)),
            // Hidden rather than scrolling sideways, so GPUI leaves sideways to the area, which goes by the gesture.
            Scrolls::BothWays => (holding_wide(area.overflow_x_hidden(), self.content), Scrollbar::new(&handle)),
        };
        let bar = bar.id((self.id.clone(), "bar")).viewport_from_layout();
        div()
            .relative()
            .child(area.test_support())
            // After the area, so what it holds acts on the wheel first.
            .child(takes_the_wheel(self.id, self.scrolls, handle))
            .child(div().absolute().inset_0().child(bar))
    }
}

/// `area`, holding `content` laid out as wide as its widest line, so the area can scroll sideways along it.
///
/// GPUI measures how far an area scrolls by what's directly in it, so `content` goes in a box that wide, and at least
/// as wide as the area, so a line's colour reaches its edge. The area is a row, so the box can be wider than it, with
/// the box at the top, since a row would stretch it to the area's height and leave nothing below to scroll to.
pub(crate) fn holding_wide(area: Stateful<Div>, content: impl IntoElement) -> Stateful<Div> {
    area.flex().items_start().child(div().flex_none().min_w_full().child(content))
}

/// Lies over the area `id`, and acts on each wheel event over it as [`turn`] says: down or up, and sideways too if it
/// `scrolls` both ways, along the way the event's gesture goes, as [`Wheel`] heard it. What it scrolls or keeps, the
/// transcript doesn't see.
fn takes_the_wheel(id: ElementId, scrolls: Scrolls, handle: ScrollHandle) -> impl IntoElement {
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal).id,
        move |_, hitbox, window, _| {
            let view = window.current_view();
            window.on_mouse_event(move |_: &ScrollEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !hitbox.should_handle_scroll(window) {
                    return;
                }
                let wheel = cx.default_global::<Wheel>();
                let (delta, offset, max) = (wheel.delta, handle.offset(), handle.max_offset());
                let to = if delta.y.abs() > delta.x.abs() {
                    scrolled(offset.y, max.y, delta.y).map(|y| point(offset.x, y))
                } else {
                    match scrolls {
                        Scrolls::BothWays if delta.x != px(0.) => {
                            scrolled(offset.x, max.x, delta.x).map(|x| point(x, offset.y))
                        }
                        Scrolls::BothWays | Scrolls::Down => return,
                    }
                };
                let flinging = wheel.flinging;
                match turn(&mut wheel.owner, &id, flinging, to) {
                    Turn::Scroll(to) => {
                        handle.set_offset(to);
                        cx.notify(view);
                        cx.stop_propagation();
                    }
                    Turn::Keep => cx.stop_propagation(),
                    Turn::Pass => {}
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

/// What an area does with a wheel event over it.
#[derive(Debug, PartialEq)]
enum Turn<T> {
    /// Scrolls to here, and keeps the event from the transcript.
    Scroll(T),
    /// Keeps the event from the transcript without scrolling.
    Keep,
    /// Leaves the event to the transcript.
    Pass,
}

/// What the area `me` does with a wheel event over it, part of a fling or not, when it would scroll `to` there, as
/// [`scrolled`] says, given what the gesture has gone to, which it can change.
fn turn<T>(owner: &mut Owner, me: &ElementId, flinging: bool, to: Option<T>) -> Turn<T> {
    match owner {
        // The gesture's first movement: the area takes the gesture if it can move, and leaves it to the transcript if
        // it can't.
        Owner::Undecided => {
            *owner = if to.is_some() { Owner::Area(me.clone()) } else { Owner::Transcript };
            to.map_or(Turn::Pass, Turn::Scroll)
        }
        Owner::Area(area) if area == me => match to {
            Some(to) => Turn::Scroll(to),
            // A fling stops at the area's end, rather than carrying the transcript on at its speed.
            None if flinging => Turn::Keep,
            // Fingers still moving, or a wheel still turning, carry on into the transcript, which keeps the gesture.
            None => {
                *owner = Owner::Transcript;
                Turn::Pass
            }
        },
        // A gesture that started on the transcript, or over another area, passes over this one.
        Owner::Area(_) | Owner::Transcript => Turn::Pass,
    }
}

/// Where an area at `offset` along one way, which goes from 0 at its top or left to `-max` at its end, scrolls to for a
/// wheel's `delta`, toward the end when it's negative. `None` when the area is already at the end the wheel is turning
/// toward.
fn scrolled(offset: Pixels, max: Pixels, delta: Pixels) -> Option<Pixels> {
    let to = (offset + delta).clamp(-max, px(0.));
    (to != offset).then_some(to)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{ElementId, Pixels, px};

    use super::{Owner, Turn, scrolled, turn};

    #[test]
    fn the_wheel_scrolls_the_area_while_it_has_room() {
        assert_eq!(scrolled(px(0.), px(100.), px(-40.)), Some(px(-40.)));
        assert_eq!(scrolled(px(-60.), px(100.), px(40.)), Some(px(-20.)));
    }

    #[test]
    fn the_wheel_stops_at_an_end_and_then_passes_on() {
        assert_eq!(scrolled(px(-80.), px(100.), px(-40.)), Some(px(-100.)), "it goes as far as the end");
        assert_eq!(scrolled(px(-100.), px(100.), px(-40.)), None, "down at the end");
        assert_eq!(scrolled(px(0.), px(100.), px(40.)), None, "up at the top");
        assert_eq!(scrolled(px(0.), px(0.), px(-40.)), None, "nothing to scroll");
        assert_eq!(scrolled(px(-50.), px(100.), px(0.)), None, "no movement");
    }

    fn me() -> ElementId {
        ElementId::Name("output".into())
    }

    /// What the area does with an event, and what the gesture has gone to after it.
    fn turned(mut owner: Owner, flinging: bool, to: Option<Pixels>) -> (Turn<Pixels>, Owner) {
        let turn = turn(&mut owner, &me(), flinging, to);
        (turn, owner)
    }

    #[test]
    fn a_gesture_goes_to_the_area_if_its_first_movement_can_scroll_it() {
        let to = Some(px(-40.));

        assert_eq!(turned(Owner::Undecided, false, to), (Turn::Scroll(px(-40.)), Owner::Area(me())));
        assert_eq!(
            turned(Owner::Undecided, false, None),
            (Turn::Pass, Owner::Transcript),
            "and to the transcript if not"
        );
    }

    #[test]
    fn at_its_end_the_area_hands_the_gesture_on_but_stops_a_fling() {
        assert_eq!(turned(Owner::Area(me()), false, Some(px(-40.))), (Turn::Scroll(px(-40.)), Owner::Area(me())));
        assert_eq!(turned(Owner::Area(me()), false, None), (Turn::Pass, Owner::Transcript));
        assert_eq!(turned(Owner::Area(me()), true, None), (Turn::Keep, Owner::Area(me())));
    }

    #[test]
    fn a_gesture_that_went_elsewhere_passes_over_the_area() {
        let other = Owner::Area(ElementId::Name("another".into()));
        for owner in [Owner::Transcript, other] {
            for flinging in [false, true] {
                assert_eq!(turned(owner.clone(), flinging, Some(px(-40.))), (Turn::Pass, owner.clone()));
            }
        }
    }
}
