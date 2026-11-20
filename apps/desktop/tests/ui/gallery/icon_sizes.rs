//! Every size of an icon, in one picture, as `look::IconSize` has them: each by its name, as a few icons on raised
//! squares as big as they are, so each one's box shows, then one beside the kind of text it goes with, and how big it
//! is, as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{Icon, IconName, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, ParentElement as _, PlatformTextSystem, Styled as _, TestSupportExt as _,
    div, px,
};
use look::{IconSize, IconSizing as _, Space, Type, Typeset as _};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "icons";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each size.
const KIND: &str = "look::IconSize";
/// Wide enough for three of the largest, apart.
const ALONE_WIDTH: f32 = 90.;
/// Wide enough for the longest of the icons beside text.
const BESIDE_WIDTH: f32 = 150.;
const COLUMNS: Columns = Columns { name: 90., sample: ALONE_WIDTH + BESIDE_WIDTH, written: 50., row: 48. };

/// An icon beside the kind of text it goes with, at each size.
fn beside(size: IconSize) -> (IconName, &'static str, Type) {
    match size {
        IconSize::Small => (IconName::Check, "Up to date", Type::Detail),
        IconSize::Regular => (IconName::TriangleAlert, "Ends in 9 days", Type::Body),
        IconSize::Large => (IconName::Bot, "Explore", Type::Body),
    }
}

fn rows(cx: &App) -> Vec<AnyElement> {
    let colors = look::colors(cx);
    let row = |&icon: &IconSize| {
        let [name, pixels, _] = icon.written();
        let place = places::id(KIND, &name);
        // Each in a place as wide as the largest, so the icons are in columns down the sheet.
        let boxed = |name: IconName| {
            let drawn = div().bg(colors.raised).child(Icon::new(name).sized(icon));
            div().w(IconSize::Large.size()).flex().justify_center().child(drawn)
        };
        let alone = h_flex()
            .w(px(ALONE_WIDTH))
            .gap(Space::Small)
            .children([IconName::Check, IconName::Folder, IconName::Close].map(boxed));
        let (beside_it, words, kind) = beside(icon);
        let beside = h_flex()
            .w(px(BESIDE_WIDTH))
            .gap(Space::Small)
            .typeset(kind)
            .text_color(colors.text_muted)
            .child(Icon::new(beside_it).sized(icon))
            .child(words);
        COLUMNS.row(name, h_flex().child(alone).child(beside).id(place).test_support(), pixels, cx)
    };
    IconSize::ALL.iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each size's icons.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let names: Vec<String> = IconSize::ALL.iter().map(|icon| icon.written()[0].clone()).collect();
    COLUMNS.draw(text_system, &[(KIND, &names)], rows)
}
