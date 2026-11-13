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
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Bounds, Context, ElementId, EntityId, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Point, Role, Size, StatefulInteractiveElement as _, Styled as _, Window, div, point, px, size,
};
use look::motion::Scrolling;
use look::{InRegion as _, Off as _, Region};

use crate::around_column;
use crate::scroll_area::hears_the_wheel;
use crate::selection::Selection;
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
/// The side of the square button back to the latest row.
const LATEST_BUTTON_SIDE: Pixels = px(28.);
/// How far the button back to the latest row rests above what's under the chat, such as the composer.
const LATEST_BUTTON_ROOM: Pixels = px(8.);
/// How far the button back to the latest row rises to where it rests as it fades in.
const LATEST_BUTTON_RISE: Pixels = px(4.);

#[derive(Clone)]
pub struct TranscriptList {
    state: Rc<RefCell<State>>,
    /// The transcript, which is drawn again when the list moves.
    view: EntityId,
    /// What's selected of the rows' text, which a press in the list starts and a drag past its edge scrolls on.
    selection: Selection,
}

impl TranscriptList {
    pub(crate) fn new(selection: Selection, cx: &mut Context<Transcript>) -> Self {
        Self { state: Rc::new(RefCell::new(State::new())), view: cx.entity_id(), selection }
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

    /// Someone opened something in `row`, or closed something if not `open`, so the list keeps that row where it is,
    /// and what they clicked stays under the pointer, as far as the session's end lets it.
    pub(crate) fn keep_still(&self, row: usize, open: bool) {
        self.state.borrow_mut().keep_still(row, open);
    }

    pub fn go_to_row(&self, row: usize) {
        self.state.borrow_mut().go_to_row(row);
    }

    /// Shows `row` with its end at the bottom of the list, and the rows before it above, until something else moves
    /// the list. A row taller than the list shows from its start, and one near the start of the session, with less
    /// than the list's height above it, shows where it is with the first row at the top.
    pub(crate) fn go_to_row_at_bottom(&self, row: usize) {
        self.state.borrow_mut().go_to_row_at_bottom(row);
    }

    /// The row at the bottom of the list: the one [`TranscriptList::go_to_row_at_bottom`] sent there, until something
    /// else moves the list, even near the start of the session, where it shows higher up, and otherwise the last row
    /// with more than a pixel of it in the list, as the last frame laid them out.
    pub(crate) fn bottom_row(&self) -> usize {
        self.state.borrow().bottom_row()
    }

    /// Whether any of the row at `row` showed in the last frame.
    pub(crate) fn shows(&self, row: usize) -> bool {
        self.state.borrow().shown.contains(&row)
    }

    /// Where the rows were drawn in the last frame, in the window.
    pub(crate) fn viewport(&self) -> Bounds<Pixels> {
        self.state.borrow().viewport
    }

    /// Scrolls down by `by`, or up for less than nothing, as the wheel would.
    pub(crate) fn scroll_down(&self, by: Pixels) {
        self.state.borrow_mut().scroll_by(-by);
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

    /// Each height `row` has been measured at, in turn.
    #[cfg(feature = "testing")]
    pub fn heights_measured(&self, row: usize) -> Vec<Pixels> {
        self.state.borrow().measures.get(&row).cloned().unwrap_or_default()
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
    /// which of those to fetch. `room_under` is what's left between the list and what's under it, such as the
    /// composer, which the button back to the latest row counts as part of its own room.
    pub(crate) fn render(
        &self,
        id: ElementId,
        mut row: impl FnMut(usize, &mut Window, &mut App) -> Option<AnyElement> + 'static,
        want: impl Fn(usize) + 'static,
        room_under: Pixels,
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
            selection: self.selection.clone(),
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
            .children(self.jump_button(id, room_under, window, cx))
            .into_any_element()
    }

    /// The button back to the latest row, which fades in once someone has scrolled away from it, and out as soon as
    /// it's clicked. It sits on the composer's left edge, which reaches a little past the column's, so it's over the
    /// edge of the chat, and out of the way of a prompt, at the column's right. The composer reaches less far in a
    /// narrow pane, so there the button covers more of the column's start.
    fn jump_button(
        &self,
        id: ElementId,
        room_under: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui_kit::Div> {
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
        let (theme, edge) = (cx.theme(), look::field_edge());
        // Its edge is lighter than a line, so it reads as something to press over whatever is under it rather than as
        // a divider. A custom button's edge is its colour whatever state it's in, so the edge stays as the pointer
        // lights it, where a secondary one's would turn to a line's.
        let style =
            ButtonCustomVariant::new(cx).color(edge).hover(theme.secondary_hover).active(theme.secondary_active);
        let list = self.clone();
        // A small button's icon, on a square a little bigger, so it's easy to hit over the chat. The kit draws a custom
        // button's colour faintly as its fill, so the pane's colour is set on the button itself, which hover and press
        // still change. Its edge and icon are set there too, over the kit's style for a button that's off, so it looks
        // the same as it fades out, off so it isn't pressed again.
        let button = Button::new((id, "jump-to-latest"))
            .custom(style)
            .small()
            .icon(IconName::ChevronDown)
            .accessibility_label("Jump to latest")
            .tooltip("Jump to latest")
            .size(LATEST_BUTTON_SIDE)
            .border_1()
            .border_color(edge)
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_click(move |_, _, cx| {
                list.scroll_to_latest(cx);
                cx.notify(list.view);
            })
            .off_while(!wanted);
        // In a box only as big as the button, which takes the pointer from what's under it, such as a tool call's line,
        // which a narrow pane has under it, so a click doesn't open the call too, but lets the wheel through to the
        // chat. The box is also the button's region, which a picture of it is cut to.
        let button = div().block_mouse_except_scroll().region(Region::LatestButton).child(button);
        // Rising to where it rests, with what the list leaves under it counted in.
        let bottom = LATEST_BUTTON_ROOM - room_under - LATEST_BUTTON_RISE * (1. - shown);
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(bottom)
                .opacity(shown)
                .child(around_column(div().flex().child(button))),
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
