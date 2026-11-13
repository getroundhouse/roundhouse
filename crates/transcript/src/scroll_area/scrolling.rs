use self::wheel::{Owner, Wheel};
use super::wheel;
use crate::find::reveal;
use crate::find::reveal::Reveal;
use crate::selection::Selection;
use gpui_kit::base::{InteractiveElementExt as _, Scrollbar};
use gpui_kit::{
    AnyElement, App, DispatchPhase, ElementId, HitboxBehavior, InteractiveElement as _, IntoElement, ParentElement,
    Pixels, RenderOnce, ScrollEvent, ScrollHandle, StatefulInteractiveElement as _, Styled, TestSupportExt as _,
    Window, canvas, div, point, px,
};

/// Holds `content`, at most `max_height` tall, scrolling it once it's taller, and once it's wider if it scrolls both
/// ways.
#[derive(IntoElement)]
pub(crate) struct ScrollArea {
    pub(super) id: ElementId,
    pub(super) max_height: Pixels,
    pub(super) scrolls: Scrolls,
    pub(super) content: AnyElement,
    /// Where the transcript's match being shown was drawn, to scroll to when it's in here.
    pub(super) reveal: Option<Reveal>,
    /// The transcript's selection, to scroll on under a drag that started in here.
    pub(super) drags: Selection,
}

/// Which ways an area scrolls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Scrolls {
    Down,
    /// Down, and sideways along its longest line.
    BothWays,
}

impl ScrollArea {
    /// `id` keeps the area's place from one frame to the next, so it's the area's alone. It scrolls on under a drag of
    /// `drags` that started in a text in here, held past its top or bottom. The text says it's in here by its area, as
    /// `crate::texts::Written::in_area` and `crate::texts::Plain::area` say.
    pub(crate) fn new(
        id: impl Into<ElementId>,
        max_height: Pixels,
        content: impl IntoElement,
        drags: &Selection,
    ) -> Self {
        let (content, drags) = (content.into_any_element(), drags.clone());
        Self { id: id.into(), max_height, scrolls: Scrolls::Down, content, reveal: None, drags }
    }

    /// Scrolls to the match being shown when it's drawn in here.
    pub(crate) fn follows(mut self, reveal: &Reveal) -> Self {
        self.reveal = Some(reveal.clone());
        self
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
            // After the area, so what it holds acts on the wheel first, and has said where a match it shows is.
            .children(
                self.reveal.map(|reveal| follows_the_match(self.id.clone(), self.scrolls, handle.clone(), reveal)),
            )
            .child(follows_the_drag(self.id.clone(), handle.clone(), self.drags))
            .child(takes_the_wheel(self.id, self.scrolls, handle))
            .child(div().absolute().inset_0().child(bar))
    }
}

/// `area`, holding `content` laid out as wide as its widest line, so the area can scroll sideways along it.
///
/// GPUI measures how far an area scrolls by what's directly in it, so `content` goes in a box that wide, and at least
/// as wide as the area, so a line's colour reaches its edge. The area is a row, so the box can be wider than it, with
/// the box at the top, since a row would stretch it to the area's height and leave nothing below to scroll to.
pub(crate) fn holding_wide<E: ParentElement + Styled>(area: E, content: impl IntoElement) -> E {
    area.flex().items_start().child(div().flex_none().min_w_full().child(content))
}

/// Draws nothing, and scrolls the area `id` to the match being shown, if it's in the area and out of its view, down
/// and, if it `scrolls` both ways, sideways. Until the match is drawn where it's scrolled to, the transcript leaves its
/// list where it is.
fn follows_the_match(id: ElementId, scrolls: Scrolls, handle: ScrollHandle, reveal: Reveal) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, (), window, _| {
            let Some(drawn) = reveal.drawn().filter(|drawn| drawn.area.as_ref() == Some(&id)) else { return };
            let (offset, max, visible) = (handle.offset(), handle.max_offset(), handle.bounds());
            let mut to = offset;
            if let Some(by) = reveal::scroll_by(visible, drawn.bounds) {
                to.y = (offset.y - by).clamp(-max.y, px(0.));
            }
            if scrolls == Scrolls::BothWays
                && let Some(by) = reveal::scroll_across(visible, drawn.bounds)
            {
                to.x = (offset.x - by).clamp(-max.x, px(0.));
            }
            if to != offset {
                handle.set_offset(to);
                reveal.moved();
                window.refresh();
            }
        },
    )
    .absolute()
    .size_0()
}

/// Draws nothing, and scrolls the area `id` down or up under a drag of `selection` that started in a text in it, for as
/// long as it's held past the area's bottom or top and the area has further to go.
fn follows_the_drag(id: ElementId, handle: ScrollHandle, selection: Selection) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, (), window, cx| {
            let now = cx.background_executor().now();
            let Some(by) = selection.scroll_area_past_edge(&id, handle.bounds(), now) else { return };
            let (offset, max) = (handle.offset(), handle.max_offset());
            let to = (offset.y - by).clamp(-max.y, px(0.));
            if to != offset.y {
                handle.set_offset(point(offset.x, to));
            }
            // The first frame only starts the clock, and one at the end has nothing to wait for.
            if to != offset.y || by == px(0.) {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .size_0()
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
