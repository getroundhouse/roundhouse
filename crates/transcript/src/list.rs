//! The list a transcript's rows are drawn in: only the rows on screen, keeping to the latest row until someone
//! scrolls away, with a scrollbar and a button that scrolls back down to the latest row.
//!
//! It's the transcript's own, rather than GPUI's list, to hold to the four rules `docs/ui.md` gives. It draws only
//! what's on screen, so a long session costs what a short one does. It keeps its place as a row and how far into
//! it, so nothing that changes a height out of sight moves what's in sight, and keeps a row in sight where it is, as
//! [`Anchor`] says, so a row in sight that changes height by itself moves nothing else on screen. And it keeps each
//! row's height as it was last measured, as [`Heights`] says, and guesses at the rest, so the scrollbar goes by the
//! whole session without any of it being measured for that. GPUI's list measures every row or only those on screen,
//! and forgets every height when its width changes.
//!
//! The kit's scrollbar and its bounce at the ends work on anything that says how far it's scrolled and how tall it
//! is, which [`TranscriptList`] does for them.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::motion::transition;
use gpui_kit::base::{ScrollBounce, ScrollbarHandle};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, IconName};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Bounds, Context, ElementId, EntityId, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Point, Role, Size, StatefulInteractiveElement as _, Styled as _, Window, div, point, px, rems, size,
};
use look::motion::Scrolling;
use look::{InRegion as _, Off as _, Region};

use crate::scroll_area::hears_the_wheel;
use crate::transcript::Transcript;

mod element;
mod heights;
mod layout;
mod state;

use element::ListElement;
#[cfg(doc)]
use heights::Heights;
#[cfg(doc)]
use layout::Anchor;
pub use state::Place;
use state::State;

/// The space above the first row and below the last.
const EDGE: Pixels = px(8.);

#[derive(Clone)]
pub struct TranscriptList {
    state: Rc<RefCell<State>>,
    /// The transcript, which is drawn again when the list moves.
    view: EntityId,
}

impl TranscriptList {
    pub(crate) fn new(cx: &mut Context<Transcript>) -> Self {
        Self { state: Rc::new(RefCell::new(State::new())), view: cx.entity_id() }
    }

    /// How many rows there are, loaded or not.
    pub fn row_count(&self) -> usize {
        self.state.borrow().heights.rows()
    }

    /// The row at the top of the list, and how far into it the list is, as the last frame laid them out. After the
    /// wheel has turned or the list has been sent to a row, it's where the list was until the next frame.
    pub fn top(&self) -> Place {
        self.state.borrow().top()
    }

    /// Makes it `rows` rows, of which the runs in `loaded` are loaded. Rows only come and go at the end.
    pub(crate) fn set_rows(&self, rows: usize, loaded: Vec<Range<usize>>) {
        self.state.borrow_mut().set_rows(rows, loaded);
    }

    /// These rows have changed, so each is to be measured again: when it's next drawn, or with the few measured in
    /// each frame if it's out of sight.
    pub(crate) fn out_of_date(&self, rows: Range<usize>) {
        self.state.borrow_mut().out_of_date(rows);
    }

    /// Every row may have changed height.
    pub(crate) fn all_out_of_date(&self) {
        self.state.borrow_mut().all_out_of_date();
    }

    /// Someone opened or closed something in `row`, so the list keeps that row where it is, and what they clicked
    /// stays under the pointer.
    pub(crate) fn keep_still(&self, row: usize) {
        self.state.borrow_mut().keep_still(row);
    }

    pub fn go_to_row(&self, row: usize) {
        self.state.borrow_mut().go_to_row(row);
    }

    /// Goes to the latest row, and keeps to it as more arrives.
    pub fn go_to_latest(&self) {
        self.state.borrow_mut().go_to_latest();
    }

    /// Scrolls down to the latest row over [`look::motion::SCROLL`], and keeps to it as more arrives. Under Reduce
    /// Motion it's there in the next frame.
    fn scroll_to_latest(&self, cx: &App) {
        self.state.borrow_mut().head_to_latest(Scrolling::set_off(cx));
    }

    /// How far down the scrollbar its thumb is, from 0 to 1, and how tall the list takes the session to be.
    #[cfg(feature = "testing")]
    pub fn thumb(&self) -> (f32, Pixels) {
        let state = self.state.borrow();
        let furthest = state.furthest();
        let fraction = if furthest > px(0.) { state.scrolled() / furthest } else { 1. };
        (fraction, furthest + state.tall())
    }

    /// How many rows are loaded, as the list was last told, which is what it has measured or will. The session's log
    /// can be a page ahead of that, since pages arrive on another thread.
    #[cfg(feature = "testing")]
    pub fn loaded_rows(&self) -> usize {
        self.state.borrow().loaded_rows()
    }

    /// How many times a row has been drawn since the transcript was opened, whether to show it or to measure it.
    #[cfg(feature = "testing")]
    pub fn rows_drawn(&self) -> u64 {
        self.state.borrow().rows_drawn
    }

    /// How many loaded rows have a height that's out of date, and are still to be measured.
    #[cfg(feature = "testing")]
    pub fn rows_to_measure(&self) -> usize {
        self.state.borrow().out_of_date_near_the_top(usize::MAX).len()
    }

    /// How tall the row at `row` was when it was last measured, or `None` if it never has been.
    #[cfg(feature = "testing")]
    pub fn row_height(&self, row: usize) -> Option<Pixels> {
        self.state.borrow().heights.measured(row)
    }

    /// Whether someone has scrolled away from the latest row.
    pub(crate) fn is_scrolled_up(&self) -> bool {
        self.state.borrow().is_scrolled_up()
    }

    /// The list, drawing each row with `row`, which gives `None` for a row that hasn't been fetched. `want` is told
    /// which of those to fetch.
    pub(crate) fn render(
        &self,
        id: ElementId,
        mut row: impl FnMut(usize, &mut Window, &mut App) -> Option<AnyElement> + 'static,
        want: impl Fn(usize) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let (rows, view) = (self.row_count(), self.view);
        // The space at each end is part of the first and last rows, so it scrolls with them.
        let spaced = move |ix: usize, window: &mut Window, cx: &mut App| {
            let content = row(ix, window, cx)?;
            let spaced = div()
                .w_full()
                .when(ix == 0, |d| d.pt(EDGE))
                .when(ix + 1 == rows, |d| d.pb(EDGE))
                .child(div().w_full().min_w_0().px_3().child(content));
            Some(spaced.into_any_element())
        };
        let element = ListElement {
            id: ElementId::from((id.clone(), "rows")),
            state: self.state.clone(),
            row: Box::new(spaced),
            want: Rc::new(want),
        };
        let bounce = ScrollBounce::new((id.clone(), "bounce"), self, element).on_scroll(move |_, cx| cx.notify(view));
        let viewport = div()
            .id((id.clone(), "viewport"))
            // Read out as a log, where what's added is news.
            .role(Role::Log)
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(bounce)
            .vertical_scrollbar(self);
        div()
            .region(Region::Transcript)
            .relative()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            // Before the rows, so a block in one knows which gesture the wheel is part of when it hears it.
            .child(hears_the_wheel())
            .child(viewport)
            .children(self.jump_button(id, window, cx))
            .into_any_element()
    }

    /// The button back to the latest row, which fades in once someone has scrolled away from it, and out as soon as
    /// it's clicked.
    fn jump_button(&self, id: ElementId, window: &mut Window, cx: &mut App) -> Option<gpui_kit::Div> {
        let wanted = self.is_scrolled_up() && !self.state.borrow().is_heading_to_latest();
        let shown = transition(
            (id.clone(), "jump-button-visibility"),
            if wanted { 1. } else { 0. },
            look::motion::fade(),
            window,
            cx,
        );
        if shown <= 0. {
            return None;
        }
        let colors = cx.theme().semantic_tokens().colors;
        let list = self.clone();
        let button = Button::new((id, "jump-to-latest"))
            .secondary()
            .icon(IconName::ArrowDown)
            .tooltip("Jump to latest")
            .rounded(cx.theme().radius_full())
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_color(colors.foreground)
            .on_click(move |_, _, cx| {
                list.scroll_to_latest(cx);
                cx.notify(list.view);
            })
            .off_while(!wanted);
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(rems(0.5 + shown * 0.5))
                .flex()
                .justify_center()
                .opacity(shown)
                .child(button),
        )
    }
}

/// What the kit's scrollbar and its bounce at the ends go by.
impl ScrollbarHandle for TranscriptList {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.state.borrow().viewport
    }

    fn offset(&self) -> Point<Pixels> {
        point(px(0.), -self.state.borrow().bar_scrolled())
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.state.borrow_mut().bar_scroll_to(-offset.y);
    }

    fn content_size(&self) -> Size<Pixels> {
        let state = self.state.borrow();
        size(state.viewport.size.width, state.bar_tall())
    }

    fn start_drag(&self) {
        self.state.borrow_mut().start_drag();
    }

    fn end_drag(&self) {
        self.state.borrow_mut().end_drag();
    }
}
