//! The rows on screen, as an element: lays them out and draws them. Only these are drawn, however long the session
//! is, and a few of the loaded rows out of sight are measured with each frame.
//!
//! The wheel isn't heard here. The bounce at the ends that this is drawn inside takes it, and scrolls through the
//! same handle the scrollbar does.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, point, px, relative, size,
};

use super::layout::{Layout, Rows, lay_out};
use super::state::State;
use crate::selection::Selection;

/// How many windows' height a turn of the wheel can move the top by and still have its place found row by row, each
/// measured as it's passed. Further than that goes by what the rows between are guessed to come to, which is what a
/// place that far off was found by.
const WALKED: f32 = 3.;
/// How many loaded rows out of sight are measured in a frame, on top of those in it. At about 40 µs a row, that's
/// 2 ms of a frame, and it gets through a page that's just loaded in six frames.
const OUT_OF_SIGHT: usize = 48;

/// Draws row `ix`, or `None` for one that hasn't been fetched.
pub(super) type DrawRow = Box<dyn FnMut(usize, &mut Window, &mut App) -> Option<AnyElement>>;

pub(super) struct ListElement {
    pub(super) id: ElementId,
    pub(super) state: Rc<RefCell<State>>,
    pub(super) row: DrawRow,
    /// Told a row in or near the window that hasn't been fetched, to fetch it.
    pub(super) want: Rc<dyn Fn(usize)>,
    /// What's selected of the rows' text, which a press on it starts and a drag past the list's edge scrolls on.
    pub(super) selection: Selection,
}

/// The rows as a frame is laying them out, each drawn as wide as the list.
struct Measuring<'a> {
    list: &'a mut ListElement,
    width: Pixels,
    window: &'a mut Window,
    cx: &'a mut App,
}

impl Rows<AnyElement> for Measuring<'_> {
    fn kept(&self, row: usize) -> Option<Pixels> {
        self.list.state.borrow().kept(row)
    }

    /// Notes the height it came out, too.
    fn draw(&mut self, row: usize) -> Option<(Pixels, AnyElement)> {
        let mut element = (self.list.row)(row, self.window, self.cx)?;
        let room = size(AvailableSpace::Definite(self.width), AvailableSpace::MinContent);
        let height = element.layout_as_root(room, self.window, self.cx).height;
        let mut state = self.list.state.borrow_mut();
        state.measured(row, height);
        #[cfg(feature = "testing")]
        {
            state.rows_drawn += 1;
        }
        Some((height, element))
    }

    /// What it was when it was last here, or what the measured rows come to on average.
    fn guess(&self, row: usize) -> Pixels {
        self.list.state.borrow().heights.height_of(row)
    }
}

impl ListElement {
    /// Lays out the rows in the window, and leaves the state at the row that came out at the top.
    fn lay_out(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) -> Layout<AnyElement> {
        let tall = bounds.size.height;
        let (rows, start) = {
            let mut state = self.state.borrow_mut();
            state.drawn_at(bounds);
            // On its way down to the latest row, it moves on by what's left with the window as tall as it is now, and
            // before the rows are laid out, so they're laid out where it's got to.
            state.head_on(|scrolling| scrolling.move_on(window, cx));
            (state.heights.rows(), state.lay_from(tall * WALKED))
        };
        let layout = lay_out(rows, start, tall, &mut Measuring { list: self, width: bounds.size.width, window, cx });
        let mut shown = layout.rows.iter().filter(|row| row.drawn.is_some()).map(|row| row.row);
        let first = shown.next();
        let shown = first.map_or(0..0, |first| first..shown.next_back().unwrap_or(first) + 1);
        let mut state = self.state.borrow_mut();
        state.laid_out(layout.top, layout.anchor, layout.end());
        (state.shown, state.last_in_sight) = (shown, layout.last_in_sight);
        drop(state);
        // The lowest of them, which is the one nearest what's already loaded when scrolling back through a session,
        // and the last a list that asked for each row as it drew it would have asked for.
        if let Some(unfetched) = layout.rows.iter().rev().find(|row| !row.fetched) {
            (self.want)(unfetched.row);
        }
        layout
    }

    /// Measures a few of the loaded rows that are out of sight and whose heights are out of date, and asks for
    /// another frame if there are more, so the guesses at them are put right without any frame waiting on it.
    fn measure_out_of_sight(&mut self, width: Pixels, window: &mut Window, cx: &mut App) {
        let (rows, more) = self.state.borrow().to_measure_out_of_sight(OUT_OF_SIGHT);
        for row in rows {
            if (Measuring { list: self, width, window, cx }).draw(row).is_none() {
                self.state.borrow_mut().up_to_date(row);
            }
        }
        if more {
            window.request_animation_frame();
        }
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
    /// The rows that show, to paint, and where the list hears the pointer.
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
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> (Vec<AnyElement>, Hitbox) {
        // A drag of the scrollbar's thumb, or one selecting text, ends when the button comes up, which the scrollbar,
        // and the list, only hear while they're drawn. If the list wasn't drawn in the last frame, such as when
        // another workspace was shown mid-drag, that drag is over.
        let drawn_last_frame =
            id.is_some_and(|id| window.with_element_state(id, |was: Option<()>, _| (was, ())).is_some());
        if !drawn_last_frame {
            self.state.borrow_mut().end_drag();
            self.selection.release();
        }
        // What the rest of the transcript was drawn from in this frame: its button back to the latest row, which
        // shows while the list is scrolled up, and the row its outline marks a prompt by, which it goes by only then.
        let read = |state: &State| state.is_scrolled_up().then(|| state.bottom_row());
        let before = read(&self.state.borrow());
        self.selection.new_frame();
        // A pointer held past the top or bottom with the button down scrolls on, for as long as it's held there.
        if let Some(by) = self.selection.scroll_past_edge(bounds, cx.background_executor().now()) {
            self.state.borrow_mut().scroll_by(-by);
            window.request_animation_frame();
        }
        let layout = self.lay_out(bounds, window, cx);
        self.measure_out_of_sight(bounds.size.width, window, cx);
        if read(&self.state.borrow()) != before {
            // Laying the rows out found another row at the bottom, or the end in sight, so those are drawn again.
            window.request_animation_frame();
        }
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let mut rows = Vec::new();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for row in layout.rows {
                if let Some(mut element) = row.drawn {
                    element.prepaint_at(bounds.origin + point(px(0.), row.top), window, cx);
                    rows.push(element);
                }
            }
        });
        // What the rows drew says where the text under the pointer is now, so a drag's end follows it as the list
        // moves under a pointer held still.
        if self.selection.follow(bounds) {
            window.request_animation_frame();
        }
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
        self.selection.hear_the_pointer(hitbox, bounds, window);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for row in rows {
                row.paint(window, cx);
            }
        });
    }
}
