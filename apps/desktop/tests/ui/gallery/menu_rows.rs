//! Every kind of row a menu has, in one picture, as the app draws it: a menu at rest, a shorter one with the pointer
//! on a row, and each kind of note at a row's end, by its name in `look::NoteKind`.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyView, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, actions, div, px, size,
};
use look::{NoteKind, Type, Typeset as _};

use super::capture::Picture;
use super::headless::{self, Pointer};
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "menu-rows";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// What the spec's table calls the type that names each kind of note.
const KIND: &str = "look::NoteKind";
const MENU_WIDTH: f32 = 280.;
const COLUMN: f32 = 320.;
const NAME_WIDTH: f32 = 90.;
const NOTE_ROW: f32 = 64.;
const MARGIN: f32 = 24.;
const HEIGHT: f32 = 300.;
/// The shorter menu, with the pointer on the row at its middle.
const POINTED: &str = "menu-pointed";

actions!(menu_rows, [MarkRead, Archive]);

/// Every kind of row: one with an icon and a key, ones with only an icon, one that's off, a label, rows to choose one
/// of with the first chosen, and one with a note.
fn every_row(menu: look::Menu, _: &mut Window, _: &mut gpui_kit::App) -> look::Menu {
    menu.row(look::menu_row("Mark as read").icon(Lucide::MailOpen).key(Box::new(MarkRead)))
        .row(look::menu_row("Rename…").icon(Lucide::Pencil))
        .row(look::menu_row("Regenerate title").icon(Lucide::PencilSparkles).off(true))
        .separator()
        .label("Effort")
        .row(look::menu_row("High").chosen(true))
        .row(look::menu_row("Max"))
        .separator()
        .row(look::menu_row("Mac Studio").icon(Lucide::Server).note("100.64.0.7", NoteKind::Detail))
}

/// A shorter menu, whose middle, where the pointer is, is a row: Rename….
fn pointed(menu: look::Menu, _: &mut Window, _: &mut gpui_kit::App) -> look::Menu {
    menu.row(look::menu_row("Mark as read").icon(Lucide::MailOpen).key(Box::new(MarkRead)))
        .row(look::menu_row("Rename…").icon(Lucide::Pencil))
        .row(look::menu_row("Archive").icon(Lucide::Archive).key(Box::new(Archive)))
}

/// A row with a note of `kind`.
fn noted(kind: NoteKind) -> impl Fn(look::Menu, &mut Window, &mut gpui_kit::App) -> look::Menu {
    move |menu, _, _| {
        let (words, note) = match kind {
            NoteKind::Detail => ("Mac Studio", "100.64.0.7"),
            NoteKind::Count => ("Mac mini", "3"),
            NoteKind::Failure => ("Old laptop", "Can't connect"),
        };
        menu.row(look::menu_row(words).icon(Lucide::Server).note(note, kind))
    }
}

#[derive(Default)]
struct Sheet {
    menus: Option<(AnyView, AnyView, Vec<AnyView>)>,
}

impl Render for Sheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (resting, pointed, notes) = self
            .menus
            .get_or_insert_with(|| {
                let notes = NoteKind::ALL.iter().map(|&kind| look::shown_menu(window, cx, noted(kind))).collect();
                (look::shown_menu(window, cx, every_row), look::shown_menu(window, cx, pointed), notes)
            })
            .clone();
        let colors = look::colors(cx);
        let menu = |id: &'static str, view: AnyView| div().id(id).test_support().w(px(MENU_WIDTH)).child(view);
        let note = |(&kind, view): (&NoteKind, AnyView)| {
            let [name] = kind.written();
            let place = places::id(KIND, &name);
            h_flex()
                .h(px(NOTE_ROW))
                .child(div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name))
                .child(div().id(place).test_support().w(px(MENU_WIDTH)).child(view))
        };
        h_flex()
            .items_start()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .child(div().w(px(COLUMN)).child(menu("menu-resting", resting)))
            .child(div().w(px(COLUMN)).child(menu(POINTED, pointed)))
            .child(v_flex().children(NoteKind::ALL.iter().zip(notes).map(note)))
    }
}

/// At the app's scale, as the screens are, with where it draws each kind of note.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    cx.update(|cx| {
        cx.bind_keys([
            gpui_kit::KeyBinding::new("cmd-shift-u", MarkRead, None),
            gpui_kit::KeyBinding::new("cmd-shift-a", Archive, None),
        ]);
    });
    let width = px(MARGIN * 2. + COLUMN * 2. + NAME_WIDTH + MENU_WIDTH);
    let names: Vec<String> = NoteKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let pointed = [(ElementId::Name(POINTED.into()), Pointer::Over)];
    headless::draw_pointing(
        &mut cx,
        size(width, px(HEIGHT)),
        None,
        |_| Sheet::default(),
        &pointed,
        |window| Places::of(window, &[(KIND, &names)]),
    )
}
