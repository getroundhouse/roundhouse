//! A row of Git's lists, one line of columns, which line up from row to row: a picture of who it is, the name, with
//! Default by the one that is, how it was connected, what else there is to say, and a menu, where there's one. A row
//! says only what isn't plain. The picture is the name's first letter in a circle, since the app fetches nothing from
//! GitHub.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use look::{IconSizing as _, Typeset as _};

use super::super::lists::badge;

/// How big the picture of who a row is is: a circle about as tall as the name's line.
const PICTURE: Pixels = px(24.);
/// How wide how it was connected is, so each row's line up with the others'.
const HOW_WIDTH: Pixels = px(140.);
/// How wide what else there is to say is, so the menus line up even where there's nothing to say.
const DETAIL_WIDTH: Pixels = px(200.);

/// What a row shows.
pub(super) struct Row {
    /// The row's, for tests to find it by, and the start of its parts'.
    pub(super) id: SharedString,
    /// The first letter of who it is, for its picture, or `None` for nobody, as for a machine logged in to no one.
    pub(super) initial: Option<char>,
    pub(super) name: String,
    /// The name is said muted, as for an account the hub holds no login for.
    pub(super) dim: bool,
    pub(super) default: bool,
    pub(super) how: Option<SharedString>,
    pub(super) detail: Option<(String, Tone)>,
    pub(super) menu: Option<AnyElement>,
    /// What's under its line: why the hub couldn't do what was asked of it, or the panel renewing its login.
    pub(super) under: Vec<AnyElement>,
}

/// How loudly a row says what else there is to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Plain,
    /// In amber, with a warning sign, for what isn't wrong yet, or may right itself.
    Warning,
    /// In red, with a warning sign, for what has to be put right.
    Problem,
}

pub(super) fn row(row: Row, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let named = div().truncate().emphasis().text_color(if row.dim { colors.text_muted } else { colors.text });
    let name = h_flex()
        .flex_1()
        .min_w_0()
        .gap(look::Space::Small)
        .child(named.child(row.name))
        .children(row.default.then(|| badge("Default", cx)));
    let detail = row.detail.map(|(said, tone)| detail(&row.id, said, tone, cx));
    // As tall as a table's row with the line under it, which is under all of the row, panel and all.
    let line = h_flex()
        .h(look::RowSize::Large.height() - look::EDGE)
        .items_center()
        .gap(look::Space::Large)
        .child(picture(row.initial, cx))
        .child(name)
        .child(column(HOW_WIDTH, cx).children(row.how))
        .child(column(DETAIL_WIDTH, cx).children(detail))
        .child(h_flex().flex_none().w(look::ControlSize::Small.height()).justify_end().children(row.menu));
    v_flex()
        .id(row.id)
        .border_b_1()
        .border_color(colors.edge)
        .child(line)
        .children(row.under)
        .test_support()
        .into_any_element()
}

fn column(width: Pixels, cx: &App) -> Div {
    div().flex_none().w(width).min_w_0().truncate().typeset(look::Type::Body).text_color(look::colors(cx).text_muted)
}

/// What else a row says, which a warning sign goes before in amber or red, said in words too, as one label, for
/// assistive apps, which can't see the colour. Tests find it by the row's `id` and `-says`.
fn detail(id: &SharedString, said: String, tone: Tone, cx: &App) -> impl IntoElement + use<> {
    let colors = look::colors(cx);
    let color = match tone {
        Tone::Plain => colors.text_muted,
        Tone::Warning => colors.warning,
        Tone::Problem => colors.error,
    };
    let sign =
        (tone != Tone::Plain).then(|| Icon::new(Lucide::TriangleAlert).sized(look::IconSize::Regular).flex_none());
    h_flex()
        .id(SharedString::from(format!("{id}-says")))
        .role(Role::Label)
        .aria_label(said.clone())
        .gap(look::Space::Small)
        .text_color(color)
        .children(sign)
        .child(div().min_w_0().truncate().child(said))
        .test_support()
}

/// Who a row is, as the first letter of their name in a circle, or an empty circle for nobody.
fn picture(initial: Option<char>, cx: &App) -> Div {
    let colors = look::colors(cx);
    let circle = div().flex_none().size(PICTURE).rounded_full().flex().items_center().justify_center();
    match initial {
        Some(initial) => circle.bg(colors.control_edge).emphasis().text_color(colors.text).child(initial.to_string()),
        None => circle.border_1().border_color(colors.control_edge),
    }
}

/// The letter a picture shows for `name`: its first, as a capital.
pub(super) fn initial(name: &str) -> Option<char> {
    name.chars().next().map(|first| first.to_ascii_uppercase())
}
