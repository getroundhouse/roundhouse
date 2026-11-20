//! Every size of a control, in one picture, as `look::ControlSize` has them: each by its name, as an outlined button
//! with words and an icon, the main button and an outlined button that's only an icon, so each one's edges show, with
//! how tall it is, the room beside its words and their kind of text, as the spec's table writes them.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{IconName, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, ParentElement as _, PlatformTextSystem, Styled as _, TestSupportExt as _,
    px,
};
use look::{ButtonKind, ControlSize, Space};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "controls";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::ControlSize";
/// Its samples wide enough for the three buttons of the largest size, apart.
const COLUMNS: Columns = Columns { name: 90., sample: 260., written: 150., row: 56. };

fn rows(cx: &App) -> Vec<AnyElement> {
    let row = |&control: &ControlSize| {
        let [name, height, padding, kind, ..] = control.written();
        let id = |what: &str| format!("{control:?}-{what}");
        let words = look::button(id("words"), ButtonKind::Outlined, control, "Add").icon(IconName::Plus);
        let main = look::button(id("main"), ButtonKind::Main, control, "Create");
        let square = look::icon_button(id("square"), ButtonKind::Outlined, control, IconName::Close);
        let sample = h_flex().w(px(COLUMNS.sample)).gap(Space::Medium).child(words).child(main).child(square);
        let sample = sample.id(places::id(KIND, &name)).test_support();
        COLUMNS.row(name, sample, format!("{height} · {padding} beside · {kind}"), cx)
    };
    ControlSize::ALL.iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each size's buttons.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let names: Vec<String> = ControlSize::ALL.iter().map(|each| each.written()[0].clone()).collect();
    COLUMNS.draw(text_system, &[(KIND, &names)], rows)
}
