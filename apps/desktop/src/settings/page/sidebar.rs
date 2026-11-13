//! Settings' sidebar: the search, then the pages under their headings.

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, ThemeStyled as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Entity, Focusable as _, InteractiveElement as _, IntoElement, Keystroke, MouseButton, OperatingSystem,
    ParentElement as _, Pixels, Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px, rems,
};
use look::{Hand as _, Typeset as _};

use super::{FIND_KEY, Page, PageId, Section};
use crate::sidebar::HEADING_HEIGHT;
use crate::workspace_pane::HEADER_HEIGHT;

/// What the search says while nothing is typed in it, and what it's called to assistive apps.
const SEARCH_SAYS: &str = "Search settings";
const SEARCH_HEIGHT: Pixels = px(28.);
const ROW_HEIGHT: Pixels = px(32.);
/// How far a page's row sets its icon in from its edge, and a heading its name, so the two line up.
const ROW_INSET: Pixels = px(8.);

/// `width` wide, as the sidebar it stands in for is, and in its colours, with the search under the strip along the
/// top, where the main sidebar's items start, and the pages scrolling under it once there are more than fit. `shown`
/// is the page showing, and pressing a page's row puts it in `chosen`.
pub(super) fn sidebar(
    search: &Entity<InputState>,
    sections: &[Section],
    shown: Option<PageId>,
    chosen: &Entity<Option<PageId>>,
    width: Pixels,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let colors = look::colors(cx);
    let headed = sections.iter().map(|section| {
        let heading = div()
            .h(HEADING_HEIGHT)
            .px(ROW_INSET)
            .flex()
            .items_center()
            .typeset(look::Type::Detail)
            .text_color(colors.text_muted)
            .child(section.heading);
        let rows = section.pages.iter().map(|page| page_row(page, shown == Some(page.id), chosen, cx));
        v_flex().mt_3().child(heading).child(v_flex().gap_0p5().children(rows))
    });
    // The rows reach a little further left than the search, which ends level with them on the right.
    v_flex()
        .flex_none()
        .w(width)
        .h_full()
        .pt(HEADER_HEIGHT)
        .pl_1p5()
        .pr_2()
        .bg(colors.sidebar)
        .border_r_1()
        .border_color(colors.edge)
        .child(div().flex_none().pl_0p5().child(search_field(search, window, cx)))
        .child(v_flex().id("settings-pages").flex_1().min_h_0().children(headed).overflow_y_scrollbar())
}

/// A field on the pane's colour, saying what it's for in faint text while it's empty, with the key that finds a
/// setting at its right end. The kit's input draws only the text, since it would leave its own room before the key,
/// and say what it's for in muted text.
fn search_field(search: &Entity<InputState>, window: &Window, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let colors = look::colors(cx);
    let focused = search.read(cx).focus_handle(cx).is_focused(window);
    let empty = search.read(cx).value().is_empty();
    // Laid under the text, so the caret is drawn over it, and a press on it reaches the text.
    let says = div().absolute().inset_0().flex().items_center().line_height(rems(1.25)).text_color(colors.text_faint);
    let text = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .when(empty, |text| text.child(says.child(SEARCH_SAYS)))
        .child(Input::new(search).appearance(false).small().px_0().aria_label(SEARCH_SAYS));
    // A press on the icon or the key puts the cursor in the search too, as a press on the text does.
    let to_focus = search.clone();
    h_flex()
        .id("settings-search")
        .w_full()
        .h(SEARCH_HEIGHT)
        .pl(px(9.))
        .pr(px(7.))
        .gap(px(9.))
        .rounded(theme.radius)
        .border_1()
        .border_color(colors.control_edge)
        .bg(colors.pane)
        .when(focused, |field| field.focus_ring_style(window, cx))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            to_focus.update(cx, |search, cx| search.focus(window, cx));
        })
        .child(Icon::new(IconName::Search).size_3p5().text_color(colors.text_muted))
        .child(text)
        .child(find_key(cx))
}

/// The key that finds a setting, on a small raised key, with what's held apart from the key pressed.
fn find_key(cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let parts = find_key_says(cx.platform_info().os);
    let pressed = parts.len().saturating_sub(1);
    let parts = parts
        .into_iter()
        .enumerate()
        .map(|(ix, part)| div().when(ix < pressed, |held| held.typeset(look::Type::KeyHeld)).child(part));
    h_flex()
        .flex_none()
        .h(px(16.))
        .px_1()
        .gap_1()
        .rounded(px(4.))
        .border_1()
        .border_color(colors.control_edge)
        .bg(colors.raised)
        .typeset(look::Type::Key)
        .line_height(px(10.))
        .text_color(colors.text_muted)
        .children(parts)
}

/// The parts of the key that finds a setting, with `secondary` read as `os` has it, and written as this build writes
/// keys, with what's held first. On a Mac each part is one symbol or letter, so the symbols for what's held can be
/// smaller than the key pressed, as they are on its keys. Elsewhere the parts are words, and come as one, with what
/// goes between them.
pub(super) fn find_key_says(os: OperatingSystem) -> Vec<String> {
    // `secondary` is a different key on each system, so it's read for the one at hand.
    let said = Keystroke::parse_with_os(FIND_KEY, os).map_or_default(|key| Kbd::format(&key));
    if cfg!(target_os = "macos") { said.chars().map(String::from).collect() } else { vec![said] }
}

/// A page's row: its icon and name, filled a little lighter than the sidebar while it's the page showing, and while
/// the pointer is on it. It tells assistive apps which page is showing, as a list's selected item.
fn page_row(page: &Page, showing: bool, chosen: &Entity<Option<PageId>>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let colors = look::colors(cx);
    let fill = colors.menu_hover;
    let (id, chosen) = (page.id, chosen.clone());
    h_flex()
        .id(id.row_id())
        .role(Role::ListBoxOption)
        .aria_label(page.name)
        .aria_selected(showing)
        .w_full()
        .h(ROW_HEIGHT)
        .px(ROW_INSET)
        .gap_3()
        .rounded(theme.radius)
        .typeset(look::Type::Body)
        .text_color(colors.text)
        .hand()
        .when(showing, |row| row.bg(fill))
        .hover(move |row| row.bg(fill))
        .on_click(move |_, _, cx| {
            chosen.update(cx, |chosen, cx| {
                *chosen = Some(id);
                cx.notify();
            });
        })
        .child(Icon::new(page.icon).size_4())
        .child(page.name)
        .test_support()
}
