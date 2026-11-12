//! The rows in sight, as an element: lays them out from the list's place down, measures the comments among them, and
//! draws them. It hears the wheel sideways, and leaves it up and down to the bounce at the ends that it's drawn in.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Element, ElementId, EntityId, GlobalElementId,
    Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels, ScrollEvent, Style, Window, point, px,
    relative, size,
};

use super::{Height, State};
use crate::file_view::LINE_HEIGHT;

/// Draws row `ix`.
type DrawRow = Box<dyn FnMut(usize, &mut Window, &mut App) -> AnyElement>;

pub(super) struct ListElement {
    id: ElementId,
    state: Rc<RefCell<State>>,
    view: EntityId,
    row: DrawRow,
}

impl ListElement {
    pub(super) fn new(id: impl Into<ElementId>, state: Rc<RefCell<State>>, view: EntityId, row: DrawRow) -> Self {
        Self { id: id.into(), state, view, row }
    }

    /// The rows from the list's place down to the bottom of `bounds`, each laid out and where it goes. A comment that
    /// comes out another height than it had is kept at its new one.
    fn lay_out(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) -> Vec<(Pixels, AnyElement)> {
        let (mut ix, mut top, rows) = {
            let mut state = self.state.borrow_mut();
            state.viewport = bounds;
            state.across = state.across.min(state.most_across());
            let down = state.down();
            state.go_down_to(down);
            (state.place.row, -state.place.offset, state.heights.len())
        };
        let (width, mut changed) = (bounds.size.width, false);
        let mut laid_out = Vec::new();
        while ix < rows && top < bounds.size.height {
            let mut element = (self.row)(ix, window, cx);
            let height = self.state.borrow().heights[ix].clone();
            let tall = match height {
                Height::Fixed(tall) => {
                    element.layout_as_root(
                        size(AvailableSpace::Definite(width), AvailableSpace::Definite(tall)),
                        window,
                        cx,
                    );
                    tall
                }
                Height::Measured(key) => {
                    let room = size(AvailableSpace::Definite(width), AvailableSpace::MinContent);
                    let tall = element.layout_as_root(room, window, cx).height;
                    let mut state = self.state.borrow_mut();
                    changed |= state.measured.insert(key, tall) != Some(tall);
                    tall
                }
            };
            laid_out.push((top, element));
            top += tall;
            ix += 1;
        }
        if changed {
            self.state.borrow_mut().add_up();
        }
        laid_out
    }
}

impl IntoElement for ListElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ListElement {
    type RequestLayoutState = ();
    /// The rows in sight, to paint, and where the wheel is heard.
    type PrepaintState = (Vec<AnyElement>, Hitbox);

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        style.min_size.height = px(0.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> (Vec<AnyElement>, Hitbox) {
        let laid_out = self.lay_out(bounds, window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let mut rows = Vec::with_capacity(laid_out.len());
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (top, mut element) in laid_out {
                element.prepaint_at(bounds.origin + point(px(0.), top), window, cx);
                rows.push(element);
            }
        });
        (rows, hitbox)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        (rows, hitbox): &mut (Vec<AnyElement>, Hitbox),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for row in rows {
                row.paint(window, cx);
            }
        });
        // Sideways only, which the bounce leaves alone. A swipe that's more down than across is the bounce's.
        let (state, view, hitbox) = (self.state.clone(), self.view, hitbox.clone());
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !hitbox.should_handle_scroll(window) {
                return;
            }
            let delta = event.delta.pixel_delta(LINE_HEIGHT);
            if delta.x.abs() <= delta.y.abs() {
                return;
            }
            let mut state = state.borrow_mut();
            let across = (state.across - delta.x).clamp(px(0.), state.most_across());
            if across != state.across {
                state.across = across;
                cx.notify(view);
            }
            cx.stop_propagation();
        });
    }
}
