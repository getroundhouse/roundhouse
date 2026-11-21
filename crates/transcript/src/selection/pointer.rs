//! What the pointer does to a selection in a transcript's list: a press starts one, a drag moves its far end, and a
//! pointer held past the list's top or bottom scrolls the list on, faster the further past it is, as
//! [`ScrollingOn`](look::motion::ScrollingOn) says. A drag that started in an area that scrolls its text, such as a
//! compaction's long summary, scrolls that area on the same way while the pointer is past its top or bottom, as the
//! kit's selection does. Past the list's edge too, both scroll, toward the same end of the chat.

use std::time::Instant;

use gpui_kit::base::GlobalState;
use gpui_kit::{
    Bounds, DispatchPhase, ElementId, Hitbox, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    Window, point, px,
};
use look::motion::ScrollingOn;

use super::places::{spot_at, text_under};
use super::{Drag, End, Selection, Unit};

impl Selection {
    /// Listens for a press in the list at `hitbox`, in `bounds`, to select the text it's on or let go of what's
    /// selected, and while one's held, for where the pointer goes and for the button coming up.
    ///
    /// A press is heard after what's drawn over the list, such as the scrollbar, and what's in its rows, so one that
    /// takes a press keeps it, and before the kit's selection, which lets go of what it had selected as a press goes
    /// down. No text in the transcript is the kit's, but a kit component can draw a text view of its own, such as an
    /// input's popover, so one on a text has the kit select nothing from there, and the two are never both selecting.
    /// Where the pointer goes and the button coming up are heard before anything else, so a drag goes on over whatever
    /// it's on.
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

    /// The button's come up, or the list it was pressed in has gone, so the drag's over, and what it selected stays.
    pub(super) fn release(&self) {
        self.inner.borrow_mut().drag = None;
    }

    /// Moves the far end of what's being dragged to the place under the pointer, or the nearest in the window to it
    /// if it's past the window's edge, as the texts drawn in this frame have it. Says whether that changed what's
    /// selected.
    pub(super) fn follow(&self, bounds: Bounds<Pixels>) -> bool {
        let (pointer, unit, chosen) = {
            let selecting = self.inner.borrow();
            let Some(drag) = selecting.drag.as_ref() else { return false };
            (drag.pointer, drag.unit, drag.chosen.clone())
        };
        let Some(chosen_at) = self.spots(&chosen) else { return false };
        let inside = point(pointer.x, pointer.y.clamp(bounds.top() + px(1.), bounds.bottom() - px(1.)));
        let under = spot_at(&self.inner.borrow().drawn, inside).map(|(spot, text)| (spot, text.clone()));
        let Some((spot, text)) = under else { return false };
        let (from, to) = self.around(spot, &text, unit);
        let (start, end) = chosen;
        let ends = if spot < chosen_at.0 {
            (end, End::at(text, from))
        } else if to < chosen_at.1 {
            (start, end)
        } else {
            (start, End::at(text, to))
        };
        let mut selecting = self.inner.borrow_mut();
        let changed = selecting.ends.as_ref() != Some(&ends);
        selecting.set(Some(ends));
        changed
    }

    fn is_dragging(&self) -> bool {
        self.inner.borrow().drag.is_some()
    }

    /// A press of the button at `position`, the `clicks`th in a row. On a text, it starts a selection there, or with
    /// `extend`, moves the far end of the one there is to there, and says so. Anywhere else, nothing's selected.
    fn press(&self, position: Point<Pixels>, clicks: usize, extend: bool) -> bool {
        let mut selecting = self.inner.borrow_mut();
        let on = text_under(&selecting.drawn, position);
        let (on_text, area) = (on.is_some(), on.and_then(|drawn| drawn.area.clone()));
        let Some((spot, text)) = spot_at(&selecting.drawn, position).filter(|_| on_text) else {
            selecting.set(None);
            selecting.drag = None;
            return false;
        };
        let text = text.clone();
        let unit = match clicks {
            0 | 1 => Unit::Character,
            2 => Unit::Word,
            _ => Unit::Paragraph,
        };
        // Extending from where what's selected starts, if its text still has a place, and otherwise starting afresh.
        let anchor = (selecting.ends.as_ref().map(|(anchor, _)| anchor.clone()))
            .filter(|anchor| extend && unit == Unit::Character && self.texts.place_of(&anchor.text).is_some());
        let chosen = if let Some(anchor) = anchor {
            selecting.set(Some((anchor.clone(), End::at(text, spot))));
            (anchor.clone(), anchor)
        } else {
            let (from, to) = self.around(spot, &text, unit);
            let chosen = (End::at(text.clone(), from), End::at(text, to));
            selecting.set(Some(chosen.clone()));
            chosen
        };
        let (scrolling, area_scrolling) = (ScrollingOn::default(), ScrollingOn::default());
        selecting.drag = Some(Drag { pointer: position, unit, chosen, scrolling, area, area_scrolling });
        true
    }

    /// The pointer's moved to `position` with the button held, over a list in `bounds`.
    fn drag_to(&self, position: Point<Pixels>, bounds: Bounds<Pixels>) {
        if let Some(drag) = self.inner.borrow_mut().drag.as_mut() {
            drag.pointer = position;
        }
        self.follow(bounds);
    }

    /// How far to scroll a list in `bounds` down, at `now`, for a pointer held past its top or bottom, if it is.
    /// Up is less than nothing. It's nothing the first frame the pointer's past it, which only starts the clock.
    pub(super) fn scroll_past_edge(&self, bounds: Bounds<Pixels>, now: Instant) -> Option<Pixels> {
        let mut selecting = self.inner.borrow_mut();
        let drag = selecting.drag.as_mut()?;
        drag.scrolling.by(drag.pointer.y, bounds.top()..bounds.bottom(), now)
    }

    /// How far to scroll the area `area`, which shows what's in `visible` of it, down, at `now`, for a drag that
    /// started in a text it scrolls and is held past its top or bottom, if it is. Up is less than nothing, and it's
    /// nothing the first frame, as for the list.
    pub(crate) fn scroll_area_past_edge(
        &self,
        area: &ElementId,
        visible: Bounds<Pixels>,
        now: Instant,
    ) -> Option<Pixels> {
        let mut selecting = self.inner.borrow_mut();
        let drag = selecting.drag.as_mut().filter(|drag| drag.area.as_ref() == Some(area))?;
        drag.area_scrolling.by(drag.pointer.y, visible.top()..visible.bottom(), now)
    }
}
