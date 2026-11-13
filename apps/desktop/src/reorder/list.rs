//! A list that scrolls, and builds only the rows drawn in sight in each frame, wherever their places are. It's the
//! app's own rather than the kit's, which builds the rows whose places it lays out in sight, one under another, so it
//! can't build a row drawn away from its place, such as one sliding out of sight. Here a row is wherever it's drawn,
//! so rows can pass each other, a row can be given less room than it takes, one can be drawn over the others, and the
//! list is as long as it's said to be in each frame. Every row's height is known before it's built, which keeps it
//! small: nothing is measured.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, Context, Div, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, ParentElement as _, Pixels, Render,
    ScrollHandle, Stateful, StatefulInteractiveElement as _, Style, Window, div, point, relative, size,
};

/// Where a row is drawn in this frame: how far down the list its top is, and how much room it has from there, which
/// is none for a row that isn't to be drawn. A row given less room than it takes is drawn whole all the same, so one
/// that's cut short clips itself.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Drawn {
    pub(crate) top: Pixels,
    pub(crate) height: Pixels,
    /// Whether it's drawn over the rows that aren't, as one being dragged past them is.
    pub(crate) above: bool,
}

/// Builds the rows it's asked for, by their indices in the list, in the order asked.
type Build = Box<dyn Fn(&[usize], &mut Window, &mut App) -> Vec<AnyElement>>;

/// A list of `view`'s rows, each where `drawn` says, as long as `end` says, scrolled to where `scroll` keeps. `rows`
/// builds those in sight, which it's given by their indices in `drawn`.
pub(crate) fn list<V: Render>(
    view: Entity<V>,
    id: impl Into<ElementId>,
    scroll: &ScrollHandle,
    drawn: Rc<[Drawn]>,
    end: Pixels,
    rows: impl Fn(&mut V, &[usize], &mut Window, &mut Context<V>) -> Vec<AnyElement> + 'static,
) -> Stateful<Div> {
    let build: Build =
        Box::new(move |in_sight, window, cx| view.update(cx, |view, cx| rows(view, in_sight, window, cx)));
    div().id(id).overflow_y_scroll().track_scroll(scroll).child(Rows { drawn, end, build })
}

/// The rows, as tall as the list is long, inside the box that scrolls them.
struct Rows {
    drawn: Rc<[Drawn]>,
    end: Pixels,
    build: Build,
}

/// Which of the rows `drawn` says show, with the list's top at `top` and what can be seen of it between `sight`'s
/// ends, in the order they're drawn: as listed, but for those drawn above the others, which come after them.
fn in_sight(drawn: &[Drawn], top: Pixels, sight: &Range<Pixels>) -> Vec<usize> {
    let shows = |row: &Drawn| {
        let from = top + row.top;
        row.height > Pixels::ZERO && from < sight.end && from + row.height > sight.start
    };
    let (above, under): (Vec<usize>, Vec<usize>) =
        (0..drawn.len()).filter(|&at| shows(&drawn[at])).partition(|&at| drawn[at].above);
    [under, above].concat()
}

impl IntoElement for Rows {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Rows {
    type RequestLayoutState = ();
    type PrepaintState = Vec<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
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
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = self.end.into();
        style.flex_shrink = 0.;
        (window.request_layout(style, [], cx), ())
    }

    /// Builds the rows in sight, and lays each out where it's drawn. What can be seen is what the box that scrolls this
    /// lets through, which is the window's mask here.
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let sight = window.content_mask().bounds;
        let in_sight = in_sight(&self.drawn, bounds.top(), &(sight.top()..sight.bottom()));
        let mut rows = (self.build)(&in_sight, window, cx);
        for (row, &at) in rows.iter_mut().zip(&in_sight) {
            let Drawn { top, height, .. } = self.drawn[at];
            let room = size(AvailableSpace::Definite(bounds.size.width), AvailableSpace::Definite(height));
            row.layout_as_root(room, window, cx);
            row.prepaint_at(point(bounds.left(), bounds.top() + top), window, cx);
        }
        rows
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        rows: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for row in rows {
            row.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::{Drawn, in_sight};

    fn drawn(top: f32, height: f32) -> Drawn {
        Drawn { top: px(top), height: px(height), above: false }
    }

    #[test]
    fn only_the_rows_drawn_where_they_can_be_seen_are_built_in_whatever_order_theyre_listed() {
        // Scrolled 100px down a window 200px tall, whose top is 50px down the window.
        let (top, sight) = (px(-50.), px(50.)..px(250.));
        let drawn = [
            drawn(0., 40.),   // Above what's in sight.
            drawn(60., 40.),  // Ends exactly where it starts.
            drawn(61., 40.),  // Its last pixel shows.
            drawn(500., 40.), // Listed here, and drawn far below.
            drawn(299., 40.), // Its first pixel shows.
            drawn(300., 40.), // Starts exactly where it ends.
            drawn(150., 0.),  // In sight, with no room.
        ];

        assert_eq!(in_sight(&drawn, top, &sight), [2, 4]);
    }

    #[test]
    fn a_row_above_the_others_is_drawn_after_them_wherever_its_listed() {
        let sight = px(0.)..px(200.);
        let above = |top: f32| Drawn { above: true, ..drawn(top, 40.) };
        let drawn = [drawn(0., 40.), above(20.), above(900.), drawn(40., 40.), above(60.), drawn(80., 40.)];

        assert_eq!(in_sight(&drawn, px(0.), &sight), [0, 3, 5, 1, 4]);
    }
}
