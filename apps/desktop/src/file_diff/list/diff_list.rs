use super::super::FileDiff;
use super::element;
use element::ListElement;
use gpui_kit::base::{ScrollBounce, ScrollbarHandle};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::{
    AnyElement, App, Bounds, Context, ElementId, EntityId, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Point, SharedString, Size, Styled as _, TestSupportExt as _, Window, div, point, px, size,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// What a comment's row counts as until it's drawn.
const GUESS: Pixels = px(120.);

/// How tall a row is.
#[derive(Clone, Debug, PartialEq)]
pub(in super::super) enum Height {
    Fixed(Pixels),
    /// As it's drawn, kept by this key.
    Measured(SharedString),
}

/// A row, and how far down into it the list's top is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in super::super) struct Place {
    pub(in super::super) row: usize,
    pub(in super::super) offset: Pixels,
}

#[derive(Clone)]
pub(in super::super) struct DiffList {
    pub(super) state: Rc<RefCell<State>>,
    /// The diff, drawn again as its code scrolls sideways.
    pub(super) view: EntityId,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) heights: Vec<Height>,
    /// Where each row starts, and where the last ends.
    pub(super) tops: Vec<Pixels>,
    pub(super) measured: HashMap<SharedString, Pixels>,
    pub(super) place: Place,
    pub(super) viewport: Bounds<Pixels>,
    /// How far the code is scrolled sideways.
    pub(super) across: Pixels,
    pub(super) code: Code,
}

/// How wide the code is, and how much room it has.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in super::super) struct Code {
    /// The longest line's width.
    pub(in super::super) widest: Pixels,
    /// How many columns of code there are, which share the width.
    pub(in super::super) columns: u8,
    /// What each column has before its code, such as its line numbers.
    pub(in super::super) gutter: Pixels,
}

impl DiffList {
    pub(in super::super) fn new(cx: &Context<FileDiff>) -> Self {
        Self { state: Rc::default(), view: cx.entity_id() }
    }

    /// Lays the list out as `heights`, with `place` at its top.
    pub(in super::super) fn set_rows(&self, heights: Vec<Height>, place: Place) {
        let mut state = self.state.borrow_mut();
        state.heights = heights;
        state.place = place;
        state.add_up();
    }

    /// Goes back to the first row, scrolled sideways to the start, as for another file.
    pub(in super::super) fn go_to_start(&self) {
        let mut state = self.state.borrow_mut();
        (state.place, state.across) = (Place::default(), px(0.));
    }

    pub(in super::super) fn top(&self) -> Place {
        self.state.borrow().place
    }

    pub(in super::super) fn set_code(&self, code: Code) {
        self.state.borrow_mut().code = code;
    }

    /// Where the list was drawn in the last frame.
    pub(in super::super) fn viewport(&self) -> Bounds<Pixels> {
        self.state.borrow().viewport
    }

    /// Scrolls the list down by `by`, or up by less than nothing.
    pub(in super::super) fn scroll_by(&self, by: Pixels) {
        let mut state = self.state.borrow_mut();
        let down = state.down();
        state.go_down_to(down + by);
    }

    /// How far the code is scrolled sideways.
    pub(in super::super) fn across(&self) -> Pixels {
        self.state.borrow().across
    }

    /// The list, drawing row `ix` with `row`.
    pub(in super::super) fn render(
        &self,
        id: impl Into<ElementId>,
        row: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> AnyElement {
        let id = id.into();
        let element = ListElement::new((id.clone(), "rows"), self.state.clone(), self.view, Box::new(row));
        let view = self.view;
        let bounce = ScrollBounce::new((id.clone(), "bounce"), self, element).on_scroll(move |_, cx| cx.notify(view));
        div()
            .id(id)
            .test_support()
            .relative()
            .size_full()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(bounce)
            .vertical_scrollbar(self)
            .into_any_element()
    }
}

impl State {
    /// Works out where each row starts, after the heights change.
    pub(super) fn add_up(&mut self) {
        let mut top = px(0.);
        self.tops.clear();
        self.tops.reserve(self.heights.len() + 1);
        for height in &self.heights {
            self.tops.push(top);
            top += match height {
                Height::Fixed(height) => *height,
                Height::Measured(key) => self.measured.get(key).copied().unwrap_or(GUESS),
            };
        }
        self.tops.push(top);
        self.place.row = self.place.row.min(self.heights.len().saturating_sub(1));
    }

    pub(super) fn tall(&self) -> Pixels {
        self.tops.last().copied().unwrap_or_default()
    }

    /// How far down the list's top is.
    pub(super) fn down(&self) -> Pixels {
        self.tops.get(self.place.row).copied().unwrap_or_default() + self.place.offset
    }

    /// Puts the list's top `down` from the first row's, within how far it can go.
    pub(super) fn go_down_to(&mut self, down: Pixels) {
        let down = down.min(self.tall() - self.viewport.size.height).max(px(0.));
        let row =
            self.tops.partition_point(|top| *top <= down).saturating_sub(1).min(self.heights.len().saturating_sub(1));
        let offset = down - self.tops.get(row).copied().unwrap_or_default();
        self.place = Place { row, offset };
    }

    /// How far the code can scroll sideways, with the list as wide as it is.
    pub(super) fn most_across(&self) -> Pixels {
        let Code { widest, columns, gutter } = self.code;
        let room = self.viewport.size.width / f32::from(columns.max(1)) - gutter;
        (widest - room).max(px(0.))
    }
}

/// What the kit's scrollbar and its bounce at the ends go by.
impl ScrollbarHandle for DiffList {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.state.borrow().viewport
    }

    fn offset(&self) -> Point<Pixels> {
        point(px(0.), -self.state.borrow().down())
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.state.borrow_mut().go_down_to(-offset.y);
    }

    fn content_size(&self) -> Size<Pixels> {
        let state = self.state.borrow();
        size(state.viewport.size.width, state.tall())
    }
}
