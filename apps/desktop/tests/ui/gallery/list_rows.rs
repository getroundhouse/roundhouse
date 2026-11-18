//! Every size of a list's row, in one picture, as `look::ListRow` draws them: a list of each size, as All files',
//! Settings' pages and Home's are, with a row chosen, one under the pointer and the rest at rest. The pointer really is
//! over each one under the pointer, one at a time, so each is drawn as the app draws it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, v_flex};
use gpui_kit::{
    AnyElement, App, Context, ElementId, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _,
    Window, div, px, size,
};
use look::{IconSizing as _, RowSize, Type, Typeset as _};

use super::capture::Picture;
use super::headless::{self, Pointer};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "list-rows";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const MARGIN: f32 = 32.;
const BETWEEN: f32 = 24.;
const LIST: f32 = 320.;
/// The rows under the pointer, by their IDs.
const POINTED: [&str; 3] = ["small-2", "regular-2", "large-2"];

/// Each list's rows: what each says, the first chosen and the third under the pointer.
fn rows(size: RowSize) -> [(&'static str, &'static str); 4] {
    match size {
        RowSize::Small => [("lib.rs", ""), ("mark.rs", ""), ("card.rs", ""), ("Cargo.toml", "")],
        RowSize::Regular => [("General", ""), ("Notifications", ""), ("Agents", ""), ("Git", "")],
        RowSize::Large => [
            ("Show PR state in the sidebar", "roundhouse"),
            ("Read settings from TOML", "signal-box"),
            ("Retry a failed setup", "signal-box"),
            ("Fix the login redirect", "roundhouse"),
        ],
    }
}

fn page_icon(name: &str) -> Lucide {
    match name {
        "General" => Lucide::Settings2,
        "Notifications" => Lucide::Bell,
        "Agents" => Lucide::Bot,
        _ => Lucide::GitBranch,
    }
}

/// A row of `size`, at `at` in its list, holding what that list's rows hold.
fn row(size: RowSize, at: usize, (first, second): (&'static str, &'static str), cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let id = format!("{}-{at}", format!("{size:?}").to_lowercase());
    let row = look::list_row(ElementId::Name(id.into()), size).chosen(at == 0);
    match size {
        RowSize::Small => row.child(look::file_icon(first, cx)).child(div().typeset(Type::Code).child(first)),
        RowSize::Regular => row.child(Icon::new(page_icon(first)).sized(look::IconSize::Large)).child(first),
        RowSize::Large => row
            .child(look::chat_mark(look::Signal::Idle, look::Size::Inline, cx))
            .child(div().flex_1().text_color(colors.text_muted).child(first))
            .child(div().typeset(Type::Detail).text_color(colors.text_muted).child(second)),
    }
    .into_any_element()
}

/// A list of rows of `size`, under its size's name, on the ground its kind of list is on.
fn list(size: RowSize, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let ground = if size == RowSize::Large { colors.pane } else { colors.sidebar };
    v_flex()
        .gap(look::Space::Small)
        .child(div().typeset(Type::Detail).text_color(colors.text_muted).child(format!("{size:?}")))
        .child(
            v_flex()
                .w(px(LIST))
                .p(look::ROWS_INSET)
                .bg(ground)
                .children(rows(size).into_iter().enumerate().map(|(at, words)| row(size, at, words, cx))),
        )
        .into_any_element()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        gpui_kit::component::h_flex()
            .size_full()
            .items_start()
            .gap(px(BETWEEN))
            .p(px(MARGIN))
            .bg(colors.pane)
            .text_color(colors.text)
            .children([RowSize::Small, RowSize::Regular, RowSize::Large].map(|size| list(size, cx)))
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    let size = size(px(MARGIN * 2. + LIST * 3. + BETWEEN * 2.), px(MARGIN * 2. + 24. + 4. * 44. + 8.));
    let pointed: Vec<(ElementId, Pointer)> =
        POINTED.iter().map(|id| (ElementId::Name((*id).into()), Pointer::Over)).collect();
    headless::draw_pointing(&mut cx, size, None, |_| Sheet, &pointed, |_| ()).0
}
