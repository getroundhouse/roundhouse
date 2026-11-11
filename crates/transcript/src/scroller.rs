//! The list a transcript's rows are drawn in: GPUI's list, which draws only the rows on screen, keeping to the
//! latest row until someone scrolls away, with a scrollbar and a button to go back to the latest row.
//!
//! It measures every row that's loaded, not just those on screen, so the scrollbar goes by the session's real
//! height. Measuring only what's been on screen counts every other row as nothing, so in a long session the thumb
//! reaches the top in a screen or two and shakes as each row is drawn, and no guess at the rest stays put either.
//! Rows not loaded yet are guessed at, as [`Unloaded`] says, which moves the thumb a little when they load.
//!
//! It's laid out as the kit's `MessageScroller` is, which it stands in for because that keeps its list to itself.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::ScrollBounce;
use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, IconName};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, EntityId, FollowMode, InteractiveElement as _, IntoElement, ListAlignment,
    ListOffset, ListState, ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div, list, px, rems,
};
use look::Off as _;

use crate::scroll_area::hears_the_wheel;
use crate::transcript::Transcript;

mod unloaded;

pub(crate) use unloaded::Unloaded;

/// The space above the first row and below the last.
const EDGE: gpui_kit::Pixels = px(8.);

/// How far past each end of the window rows are laid out, so one is ready before it scrolls in.
const OVERDRAW: gpui_kit::Pixels = px(400.);
/// How long the button to the latest row takes to come and go.
const JUMP_BUTTON: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub struct Scroller {
    list: ListState,
    unloaded: Unloaded,
    /// Rows have changed since the list was last drawn, so what they come to on average is only known once it has
    /// been, a frame on.
    unsettled: Rc<Cell<bool>>,
    /// The transcript, which is drawn again when the list moves.
    view: EntityId,
}

impl Scroller {
    pub(crate) fn new(rows: usize, cx: &mut Context<Transcript>) -> Self {
        let list = ListState::new(rows, ListAlignment::Top, OVERDRAW).measure_all();
        list.set_follow_mode(FollowMode::Tail);
        let view = cx.entity_id();
        // Later, since the list is still borrowed while it tells of a scroll.
        list.set_scroll_handler(move |_, _, cx| cx.defer(move |cx| cx.notify(view)));
        Self { list, unloaded: Unloaded::default(), unsettled: Rc::default(), view }
    }

    pub fn item_count(&self) -> usize {
        self.list.item_count()
    }

    /// The row at the top of the list, and how far into it the list is.
    pub fn logical_scroll_top(&self) -> ListOffset {
        self.list.logical_scroll_top()
    }

    /// Adds or drops rows at the end, to make `rows` of them.
    pub(crate) fn set_rows(&self, rows: usize) {
        let have = self.list.item_count();
        if rows == have {
            return;
        }
        self.list.splice(rows.min(have)..have, rows.saturating_sub(have));
        self.unsettled.set(true);
        self.unloaded.set_rows(rows);
        // The space under the last row is part of that row, so the row that was last, or now is, has changed height.
        for last in [have, rows].into_iter().filter_map(|count| count.checked_sub(1)).filter(|last| *last < rows) {
            self.list.remeasure_items(last..last + 1);
        }
    }

    /// Measures these rows again when they're next drawn, keeping the list where it is.
    pub(crate) fn remeasure_items(&self, rows: Range<usize>) {
        self.unsettled.set(true);
        self.list.remeasure_items(rows.start.min(self.item_count())..rows.end.min(self.item_count()));
    }

    /// Measures every row again, keeping the list as far through the row at its top as it is.
    pub(crate) fn remeasure(&self) {
        self.unsettled.set(true);
        self.list.remeasure();
    }

    pub fn scroll_to_item(&self, row: usize) {
        if row < self.item_count() {
            self.list.scroll_to(ListOffset { item_ix: row, offset_in_item: px(0.) });
        }
    }

    /// Goes to the latest row, and keeps to it as more arrives.
    pub fn scroll_to_end(&self) {
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    /// How far down the scrollbar its thumb is, from 0 to 1, and how tall the list takes the session to be.
    #[cfg(feature = "testing")]
    pub fn thumb(&self) -> (f32, gpui_kit::Pixels) {
        let (down, furthest) = (-self.list.scroll_px_offset_for_scrollbar().y, self.list.max_offset_for_scrollbar().y);
        let fraction = if furthest > px(0.) { down / furthest } else { 1. };
        (fraction, furthest + self.list.viewport_bounds().size.height)
    }

    /// Whether someone has scrolled away from the latest row.
    pub(crate) fn is_scrolled_up(&self) -> bool {
        self.list.max_offset_for_scrollbar().y > px(0.)
            && !self.list.is_following_tail()
            && !self.list.is_scrolled_to_end().unwrap_or(false)
    }

    /// The list, drawing each row with `row`.
    pub(crate) fn render(
        &self,
        id: ElementId,
        mut row: impl FnMut(usize, &Unloaded, &mut Window, &mut App) -> AnyElement + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.guess_again();
        if self.unsettled.replace(false) {
            let view = self.view;
            window.on_next_frame(move |_, cx| cx.notify(view));
        }
        let (rows, unloaded, view) = (self.item_count(), self.unloaded.clone(), self.view);
        // The list takes its height from its rows alone, so the space at each end is part of the first and last.
        let rows = list(self.list.clone(), move |ix, window, cx| {
            div()
                .w_full()
                .when(ix == 0, |d| d.pt(EDGE))
                .when(ix + 1 == rows, |d| d.pb(EDGE))
                .child(div().w_full().min_w_0().px_3().child(row(ix, &unloaded, window, cx)))
                .into_any_element()
        })
        .size_full()
        .min_h_0();
        // The bounce scrolls the list itself, past the list's own word of a scroll.
        let bounce =
            ScrollBounce::new((id.clone(), "bounce"), &self.list, rows).on_scroll(move |_, cx| cx.notify(view));
        let viewport = div()
            .id((id.clone(), "viewport"))
            // Read out as a log, where what's added is news.
            .role(Role::Log)
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(bounce)
            .vertical_scrollbar(&self.list);
        div()
            .id("transcript")
            .relative()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            // Before the rows, so a block in one knows which gesture the wheel is part of when it hears it.
            .child(hears_the_wheel())
            .child(viewport)
            .children(self.jump_button(id, window, cx))
            .test_support()
            .into_any_element()
    }

    /// Draws the rows not loaded yet again if what the loaded ones come to on average has moved well away from what
    /// they were drawn at. The list's height is only known once it's taller than its window.
    fn guess_again(&self) {
        let furthest = self.list.max_offset_for_scrollbar().y;
        if furthest <= px(0.) {
            return;
        }
        let height = furthest + self.list.viewport_bounds().size.height;
        for rows in self.unloaded.reestimate(self.item_count(), height) {
            self.list.remeasure_items(rows);
        }
    }

    /// The button back to the latest row, which fades in once someone has scrolled away from it.
    fn jump_button(&self, id: ElementId, window: &mut Window, cx: &mut App) -> Option<gpui_kit::Div> {
        let wanted = self.is_scrolled_up();
        let shown = transition(
            (id.clone(), "jump-button-visibility"),
            if wanted { 1. } else { 0. },
            Transition::new(JUMP_BUTTON),
            window,
            cx,
        );
        if shown <= 0. {
            return None;
        }
        let colors = cx.theme().semantic_tokens().colors;
        let scroller = self.clone();
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
                scroller.scroll_to_end();
                cx.notify(scroller.view);
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
