use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{
    App, Context, IntoElement, MouseButton, ParentElement as _, Render, Styled as _, TestAppContext, Window, actions,
    div, point, px,
};

use super::menu::{Entry, in_order};
use super::row::Does;
use super::style::{AROUND, CORNER, ROW, ROW_CORNER};
use super::{Menu, RightClickMenu as _, menu, menu_row};
use crate::Input;
use crate::control_sizes::ControlSize;

#[test]
fn a_menus_rows_are_as_tall_as_a_dialogs_buttons() {
    assert_eq!(ROW.height(), ControlSize::Large.height());
}

#[test]
fn a_rows_corners_sit_evenly_inside_the_menus() {
    assert_eq!(CORNER.radius() - AROUND.size(), ROW_CORNER.radius());
}

#[test]
fn the_row_under_the_pointer_is_washed_as_a_borderless_button_is() {
    let colors = crate::colors::Colors::dark();
    let style = super::style(&colors);
    let button = crate::theme::kit_colors(&colors)
        .into_iter()
        .find(|(key, _)| *key == "button.ghost.hover.background")
        .map(|(_, value)| value);
    assert_eq!(style.hover.map(crate::theme::hex), button);
}

#[test]
fn the_specs_table_has_every_kind_of_note_in_order() {
    let table: Vec<[String; 1]> = crate::spec_table::rows("**Every kind of note at a menu's row's end, by its name.**");

    let kinds: Vec<[String; 1]> = super::NoteKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of note, against crates/look/src/menus");
}

#[test]
fn under_touch_the_rows_that_copy_come_first_and_with_a_pointer_stay_where_theyre_put() {
    let link = || {
        let (open, copy, go) = (menu_row("Open on GitHub"), menu_row("Copy link").copies(), menu_row("Go to it"));
        vec![Entry::Row(Box::new(open)), Entry::Row(Box::new(copy)), Entry::Separator, Entry::Row(Box::new(go))]
    };
    // Each entry, as whether it's a row that copies, or none where it's a line.
    let copying = |entries: Vec<Entry>| -> Vec<Option<bool>> {
        let copies = |entry: &Entry| match entry {
            Entry::Row(row) => Some(row.does == Does::Copies),
            Entry::Separator | Entry::Label(_) => None,
        };
        entries.iter().map(copies).collect()
    };

    assert_eq!(copying(in_order(link(), Input::Touch)), [Some(true), Some(false), None, Some(false)]);
    assert_eq!(copying(in_order(link(), Input::Pointer)), [Some(false), Some(true), None, Some(false)]);
}

#[gpui_kit::test]
fn a_copy_button_shown_under_the_pointer_isnt_drawn_under_touch(cx: &mut gpui_kit::TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::theme::apply);

    let shown = |cx: &mut gpui_kit::App| crate::copy_button_shown_while_over("row", "copy", "it", "Copy", cx).is_some();
    assert!(cx.update(shown), "With a pointer, the button shows under it");
    cx.update(|cx| Input::Touch.choose(cx));
    assert!(!cx.update(shown), "Under touch, nothing rests on it, so there's no button");
}

actions!(menus_tests, [Archive]);

/// Whether each row of the menus `build` fills shows its key, in the order they're filled.
type Shown = Rc<RefCell<Vec<bool>>>;

/// Fills a menu with a row that has a key, and notes whether it kept it.
fn archiving(shown: &Shown) -> impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static {
    let shown = shown.clone();
    move |menu, _, _| {
        let menu = menu.row(menu_row("Archive").key(Box::new(Archive)));
        let rows = menu.entries.iter().filter_map(|entry| match entry {
            Entry::Row(row) => Some(row.key.is_some()),
            Entry::Separator | Entry::Label(_) => None,
        });
        shown.borrow_mut().extend(rows);
        menu
    }
}

/// Something to right-click, whose menu has a row with a key.
struct Archivable(Shown);

impl Render for Archivable {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(div().size(px(100.)).right_click_menu(archiving(&self.0)))
    }
}

/// A right-click menu is a shortcut of its own, so its rows show no keys, as `docs/spec/components.md` says. A menu
/// that drops from a button shows a row's key, but only with a pointer, as every key shows.
#[gpui_kit::test]
fn a_right_click_menus_rows_show_no_keys_and_a_buttons_show_them_with_a_pointer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::theme::apply);
    let shown = Shown::default();
    let (_, cx) = cx.add_window_view(|_, _| Archivable(shown.clone()));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let press = point(px(10.), px(10.));

    cx.simulate_mouse_down(press, MouseButton::Right, gpui_kit::Modifiers::default());
    cx.simulate_mouse_up(press, MouseButton::Right, gpui_kit::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(PopupMenu::build(window, cx, menu(archiving(&shown)))));
    cx.update(|_, cx| Input::Touch.choose(cx));
    cx.update(|window, cx| drop(PopupMenu::build(window, cx, menu(archiving(&shown)))));

    assert_eq!(*shown.borrow(), [false, true, false], "right-clicked, from a button, and from a button under touch");
}
