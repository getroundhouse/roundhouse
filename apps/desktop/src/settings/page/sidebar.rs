//! Settings' sidebar: the search, then the pages under their headings.

use gpui_kit::component::input::InputState;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Icon, IconName, v_flex};
use gpui_kit::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Styled as _, Window, div,
};
use look::{IconSizing as _, Typeset as _};

use super::{FIND_KEY, Page, PageId, Section};
use crate::app::TOP_STRIP_HEIGHT;

/// What the search says while nothing is typed in it, and what it's called to assistive apps.
/// What the search says it's for, while nothing is typed in it, and what assistive apps call it.
pub(super) const SEARCH_SAYS: &str = "Search settings";
/// How far a page's row sets its icon in from its edge, and a heading its name, so the two line up.
const ROW_INSET: look::Space = look::Space::Small;

/// `width` wide, as the sidebar it stands in for is, and in its colours, with the search under the strip along the
/// top, where the main sidebar's items start, and the pages scrolling under it once there are more than fit. `shown`
/// is the page showing, and pressing a page's row puts it in `chosen`.
pub(super) fn sidebar(
    search: &Entity<InputState>,
    sections: &[Section],
    shown: Option<PageId>,
    chosen: &Entity<Option<PageId>>,
    width: Pixels,
    _window: &Window,
    cx: &App,
) -> impl IntoElement {
    let colors = look::colors(cx);
    let headed = sections.iter().map(|section| {
        let heading = div()
            .h(look::RowSize::Small.height())
            .px(ROW_INSET)
            .flex()
            .items_center()
            .typeset(look::Type::Detail)
            .text_color(colors.text_muted)
            .child(section.heading);
        let rows = section.pages.iter().map(|page| page_row(page, shown == Some(page.id), chosen, cx));
        v_flex().mt(look::Space::Medium).child(heading).child(v_flex().gap(look::Space::XSmall).children(rows))
    });
    // The rows' fills and the search line up on both sides.
    v_flex()
        .flex_none()
        .w(width)
        .h_full()
        .pt(TOP_STRIP_HEIGHT)
        .px(look::Space::Small)
        .bg(colors.sidebar)
        .border_r_1()
        .border_color(colors.edge)
        .child(search_field(search))
        .child(v_flex().id("settings-pages").flex_1().min_h_0().children(headed).overflow_y_scrollbar())
}

/// The search, with the key that finds a setting at its end. It's a regular field, as tall as a setting's.
fn search_field(search: &Entity<InputState>) -> impl IntoElement {
    look::field("settings-search", search, look::FieldKind::Boxed, look::ControlSize::Regular)
        .icon(IconName::Search)
        .after(look::key(look::KeyKind::Capped, FIND_KEY))
        .accessibility_label(SEARCH_SAYS)
}

/// A page's row: its icon and name, chosen while it's the page showing. It tells assistive apps which page is showing,
/// as a list's selected item.
fn page_row(page: &Page, showing: bool, chosen: &Entity<Option<PageId>>, cx: &App) -> impl IntoElement {
    let (id, chosen) = (page.id, chosen.clone());
    look::list_row(id.row_id(), look::RowSize::Regular)
        .listed(page.name)
        .chosen(showing)
        .on_click(move |_, _, cx| {
            chosen.update(cx, |chosen, cx| {
                *chosen = Some(id);
                cx.notify();
            });
        })
        .child(Icon::new(page.icon).sized(look::IconSize::Large))
        .child(div().text_color(look::colors(cx).text).child(page.name))
}
