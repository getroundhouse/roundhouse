//! Every kind of text the app sets, in one picture, as `look::Type` has them: each by its name, set as itself, with
//! its font, its size and its weight as the spec's table writes them.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::{
    AnyElement, App, InteractiveElement as _, ParentElement as _, PlatformTextSystem, Styled as _, TestSupportExt as _,
    div, px,
};
use look::{Type, Typeset as _};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "type";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::Type";
const COLUMNS: Columns = Columns { name: 110., sample: 320., written: 220., row: 44. };

/// Something the app might say in each kind.
fn sample(kind: Type) -> &'static str {
    match kind {
        Type::PageTitle => "Notifications",
        Type::Heading => "Start a workspace",
        Type::Body => "Run the tests and fix what fails",
        Type::Detail => "2 minutes ago · 3 files",
        Type::Code => "cargo test --workspace",
        Type::Key => "F",
        Type::KeyHeld => "⌘",
    }
}

fn rows(cx: &App) -> Vec<AnyElement> {
    let row = |&kind: &Type| {
        let [name, font, size, weight, _] = kind.written();
        let place = places::id(KIND, &name);
        let column = div().w(px(COLUMNS.sample));
        let set = match kind {
            Type::Code => column.typeset_code(cx),
            kind @ (Type::PageTitle | Type::Heading | Type::Body | Type::Detail | Type::Key | Type::KeyHeld) => {
                column.typeset(kind)
            }
        };
        let shown = set.child(sample(kind)).id(place).test_support();
        COLUMNS.row(name, shown, format!("{font} · {size} · {weight}"), cx)
    };
    Type::ALL.iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each kind's sample.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let names: Vec<String> = Type::ALL.iter().map(|each| each.written()[0].clone()).collect();
    COLUMNS.draw(text_system, &[(KIND, &names)], rows)
}
