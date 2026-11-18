//! A row of an account page's lists, such as Agents' and Git's: one line of columns, which line up from row to row: a
//! picture of who or what it is, the name, with Default by the one that is, how it was connected, what else there is to
//! say, and a menu, where there's one. A row says only what isn't plain. Its menu opens under its button and towards
//! the left, so it stays inside the page's column, on every account page. A row can have a card, for what its one line
//! has no room for.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled as _, TestSupportExt as _, Window, div, px,
};
use look::{IconSizing as _, Shaped as _, Typeset as _};

use super::lists::default_badge;

/// How wide an account page's lists are, so a row has no more room than what it says needs.
pub(super) const COLUMN_WIDTH: Pixels = px(720.);

/// How big a row's picture is: about as tall as the name's line.
const PICTURE: Pixels = px(24.);

/// How wide a page's middle columns are, so each row's line up with the others' on the page.
#[derive(Clone, Copy)]
pub(super) struct Columns {
    /// How it was connected.
    pub(super) how: Pixels,
    /// What else there is to say, so the menus line up even where there's nothing to say.
    pub(super) detail: Pixels,
}

/// What a row shows.
pub(super) struct Row {
    /// The row's, for tests to find it by, and the start of its parts'.
    pub(super) id: SharedString,
    pub(super) picture: Picture,
    pub(super) name: String,
    /// The name is said muted, as for an account the hub holds no login for.
    pub(super) dim: bool,
    pub(super) default: bool,
    pub(super) how: Option<SharedString>,
    pub(super) detail: Option<Detail>,
    pub(super) menu: Option<AnyElement>,
    pub(super) card: Option<Card>,
    /// What's under its line: why the hub couldn't do what was asked of it, or the panel renewing its login.
    pub(super) under: Vec<AnyElement>,
}

/// What a row's card says: what its one line has no room for. It shows under the line, with its left edge on the
/// row's, once the pointer has rested on the line as long as before a tooltip, or at once as the keyboard's focus comes
/// to the line, which it's one of Tab's stops for, before its menu.
pub(super) struct Card {
    /// What it says, in words, which assistive apps are told as what the line is, whether or not it shows.
    pub(super) said: String,
    /// What it shows, made each time it's drawn open.
    pub(super) shown: Box<dyn FnOnce(&App) -> AnyElement>,
}

/// Who or what a row is, at its start.
#[derive(Clone, Copy)]
pub(super) enum Picture {
    /// The first letter of who it is, as a capital, in a circle, as for a GitHub account, since the app fetches nothing
    /// from GitHub.
    Initial(char),
    /// An empty circle, for nobody, as for a machine logged in to no one.
    Nobody,
    /// What it is, as an icon in a square, as for an agent's account.
    Mark(Lucide),
}

impl Picture {
    /// The first letter of `name`, as a capital, or nobody for no name.
    pub(super) fn of(name: &str) -> Picture {
        name.chars().next().map_or(Picture::Nobody, |first| Picture::Initial(first.to_ascii_uppercase()))
    }
}

/// What else a row says.
pub(super) enum Detail {
    /// Words, as loudly as the tone says.
    Said(String, Tone),
    /// Something drawn, such as how much of each limit is left, which says itself to assistive apps.
    Drawn(AnyElement),
}

/// How loudly a row says what else there is to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Plain,
    /// Faint, for what's said only so an empty column isn't taken to mean something, such as that nothing is known.
    Faint,
    /// In amber, with a warning sign, for what isn't wrong yet, or may right itself.
    Warning,
    /// In red, with a warning sign, for what has to be put right.
    Problem,
}

pub(super) fn row(row: Row, columns: Columns, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    // What the line is, to assistive apps, when it has a card, which they're told of as it takes the focus.
    let label =
        [Some(row.name.clone()), row.default.then(|| "Default".to_string()), row.how.as_ref().map(ToString::to_string)];
    let label = label.into_iter().flatten().collect::<Vec<_>>().join(", ");
    let named = div().truncate().emphasis().text_color(if row.dim { colors.text_muted } else { colors.text });
    let name = h_flex()
        .flex_1()
        .min_w_0()
        .gap(look::Space::Small)
        .child(named.child(row.name))
        .children(row.default.then(|| default_badge_of(&row.id)));
    let detail = row.detail.map(|detail| match detail {
        Detail::Said(said, tone) => said_as(&row.id, said, tone, cx).into_any_element(),
        Detail::Drawn(drawn) => drawn,
    });
    // As tall as a table's row with the line under it, which is under all of the row, panel and all, so the pointer
    // anywhere on it is on what its card is for.
    let said = h_flex()
        .flex_1()
        .min_w_0()
        .h(look::RowSize::Large.height() - look::EDGE)
        .items_center()
        .gap(look::Space::Large)
        .child(picture(row.picture, cx))
        .child(name)
        .child(column(columns.how, cx).children(row.how))
        .child(column(columns.detail, cx).children(detail));
    let said = match row.card {
        Some(card) => with_card(&row.id, said, label, card),
        None => said.into_any_element(),
    };
    let line = h_flex()
        .items_center()
        .gap(look::Space::Large)
        .child(said)
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

/// `said`, a row's line but its menu, with the row's `card`, which is the app's hover card, so it's timed as every
/// other is. Assistive apps are told the line as `label`, with what the card says.
fn with_card(id: &SharedString, said: Div, label: String, card: Card) -> AnyElement {
    look::hover_card(SharedString::from(format!("{id}-card")))
        .anchor(Anchor::TopLeft)
        .flips_to_fit()
        .trigger_style(StyleRefinement::default().flex_1().min_w_0())
        .on_focus(Role::Group, label, card.said)
        .trigger(said)
        .content(move |_, _, cx| (card.shown)(cx))
        .into_any_element()
}

/// `Default`, by the name of the default, which tests find by the row's `id` and `-default`.
fn default_badge_of(id: &SharedString) -> look::Badge {
    default_badge().id(SharedString::from(format!("{id}-default")))
}

fn column(width: Pixels, cx: &App) -> Div {
    div().flex_none().w(width).min_w_0().truncate().typeset(look::Type::Body).text_color(look::colors(cx).text_muted)
}

/// What else a row says, which a warning sign goes before in amber or red, said in words too, as one label, for
/// assistive apps, which can't see the colour. Tests find it by the row's `id` and `-says`.
fn said_as(id: &SharedString, said: String, tone: Tone, cx: &App) -> impl IntoElement + use<> {
    let colors = look::colors(cx);
    let color = match tone {
        Tone::Plain => colors.text_muted,
        Tone::Faint => colors.text_faint,
        Tone::Warning => colors.warning,
        Tone::Problem => colors.error,
    };
    let warns = matches!(tone, Tone::Warning | Tone::Problem);
    let sign = warns.then(|| Icon::new(Lucide::TriangleAlert).sized(look::IconSize::Regular).flex_none());
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

/// Who or what a row is: the first letter of their name in a circle, an empty circle for nobody, or a mark in a square.
fn picture(picture: Picture, cx: &App) -> Div {
    let colors = look::colors(cx);
    let tile = div().flex_none().size(PICTURE).flex().items_center().justify_center();
    match picture {
        Picture::Initial(initial) => {
            tile.rounded_full().bg(colors.control_edge).emphasis().text_color(colors.text).child(initial.to_string())
        }
        Picture::Nobody => tile.rounded_full().border_1().border_color(colors.control_edge),
        Picture::Mark(icon) => tile
            .corners(look::Corner::Control)
            .bg(colors.control_edge)
            .child(Icon::new(icon).sized(look::IconSize::Large).text_color(colors.text_muted)),
    }
}

/// A row's `…` menu of `items`, which opens under its button and towards the left, so it stays inside the page's
/// column. Its button is known to tests by `id`, and to assistive apps as More for `name`, and it's off while `off`.
pub(super) fn menu(
    id: SharedString,
    name: &str,
    off: bool,
    items: impl Fn(look::Menu, &mut Window, &mut App) -> look::Menu + 'static,
) -> AnyElement {
    look::icon_button(id, look::ButtonKind::Borderless, look::ControlSize::Small, IconName::Ellipsis)
        .accessibility_label(format!("More for {name}"))
        .off(off)
        .menu(Anchor::TopRight, items)
        .into_any_element()
}
