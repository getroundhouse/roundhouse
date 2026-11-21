use gpui_kit::component::menu::{ContextMenu, ContextMenuExt, PopupMenu};
use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, DismissEvent, FocusHandle, IntoElement, ParentElement, Pixels,
    SharedString, Styled, WeakEntity, Window,
};

use super::row::Does;
use super::{MenuRow, menu_row};
use crate::Input;

/// What fills a menu, built each time it opens: its rows, the lines between groups of them, and the labels over some.
#[must_use]
pub struct Menu {
    kit: PopupMenu,
    open: WeakEntity<PopupMenu>,
    keys: Keys,
    input: Input,
    entries: Vec<Entry>,
}

/// Whether a menu's rows show their keys: every menu's do but a right-click menu's, which is a shortcut of its own, and
/// only with a pointer, as every key shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keys {
    Shown,
    Hidden,
}

/// What a menu holds, in the order it's given, until the menu is built.
pub(super) enum Entry {
    Row(Box<MenuRow>),
    Separator,
    Label(SharedString),
}

impl Menu {
    /// The menu as it's open, to know when it closes.
    pub fn open(&self) -> OpenMenu {
        OpenMenu(self.open.clone())
    }

    /// A row, which [`crate::menu_row`] makes.
    pub fn row(mut self, row: MenuRow) -> Self {
        let row = if self.keys == Keys::Hidden || self.input == Input::Touch { row.without_key() } else { row };
        self.entries.push(Entry::Row(Box::new(row)));
        self
    }

    /// A line between one group of rows and the next.
    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    /// A label over the rows that follow, such as Effort over the model picker's efforts.
    pub fn label(mut self, words: impl Into<SharedString>) -> Self {
        self.entries.push(Entry::Label(words.into()));
        self
    }

    /// Finds the keys its rows show, and does what they do, from where `focus` is, such as the field a menu beside it
    /// is for, rather than from the menu.
    pub fn keys_from(mut self, focus: FocusHandle) -> Self {
        self.kit = self.kit.action_context(focus);
        self
    }

    /// Scrolls once its rows would make it taller than half the window, or 450px, and opens on the row that's chosen,
    /// for a long list to choose one from, such as dictation's languages.
    pub fn scrolls(mut self) -> Self {
        self.kit = self.kit.scrollable(true);
        self
    }

    /// Exactly `width` wide, such as as wide as what opens it.
    pub fn width(mut self, width: Pixels) -> Self {
        self.kit = self.kit.min_w(width).max_w(width);
        self
    }
}

/// A menu `build` fills each time it opens, in the form the kit opens one in, for something that opens it but isn't
/// one of `look`'s, such as the kit's own button.
pub fn menu(
    build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    built(build, Keys::Shown)
}

fn built(
    build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static,
    keys: Keys,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |kit, window, cx| {
        let open = cx.entity().downgrade();
        let input = Input::of(cx);
        build(Menu { kit, open, keys, input, entries: Vec::new() }, window, cx).kit()
    }
}

impl Menu {
    /// The kit's menu, with what it holds. Under touch, the rows that copy come first, since a long press opens Copy
    /// first on anything, as a message's does in Messages, as `docs/spec/components.md` says.
    fn kit(self) -> PopupMenu {
        let Menu { kit, input, entries, .. } = self;
        in_order(entries, input).into_iter().fold(kit, |kit, entry| match entry {
            Entry::Row(row) => kit.item(row.kit()),
            Entry::Separator => kit.separator(),
            Entry::Label(words) => kit.label(words),
        })
    }
}

/// `entries` in the order a menu shows them under `input`: as they were given, but under touch the rows that copy
/// come first, in the order they were given.
pub(super) fn in_order(entries: Vec<Entry>, input: Input) -> Vec<Entry> {
    match input {
        Input::Pointer => entries,
        Input::Touch => {
            let copies = |entry: &Entry| matches!(entry, Entry::Row(row) if row.does == Does::Copies);
            let (first, rest): (Vec<_>, Vec<_>) = entries.into_iter().partition(copies);
            first.into_iter().chain(rest).collect()
        }
    }
}

/// A menu that's open, which [`Menu::open`] gives, to know when it closes.
#[derive(Clone)]
pub struct OpenMenu(WeakEntity<PopupMenu>);

impl OpenMenu {
    /// The kit's menu, while it's open.
    pub(crate) fn kit(&self) -> Option<gpui_kit::Entity<PopupMenu>> {
        self.0.upgrade()
    }

    /// Whether it's still open, since it closes once nothing holds it.
    pub fn is_open(&self) -> bool {
        self.0.upgrade().is_some()
    }

    /// Whether it's the same menu as `other`.
    pub fn is(&self, other: &OpenMenu) -> bool {
        self.0.entity_id() == other.0.entity_id()
    }

    /// Does `closed` once it closes, whether it's dismissed or goes without that, as it does when what opened it is no
    /// longer drawn.
    pub fn on_close<V: 'static>(&self, cx: &mut Context<V>, closed: impl Fn(&mut V, &mut Context<V>) + 'static) {
        let Some(menu) = self.0.upgrade() else { return };
        let closed = std::rc::Rc::new(closed);
        let dismissed = closed.clone();
        cx.subscribe(&menu, move |view, _, _: &DismissEvent, cx| dismissed(view, cx)).detach();
        cx.observe_release(&menu, move |view, _, cx| closed(view, cx)).detach();
    }
}

/// Opens a menu where something is right-clicked, or pressed long under touch.
pub trait RightClickMenu: ContextMenuExt + Sized {
    /// Opens the menu `build` fills where this is right-clicked. Its rows show no keys, as `components.md` says, since
    /// the menu is a shortcut of its own, so a row given one shows it only in a menu that drops from a button.
    #[track_caller]
    fn right_click_menu(self, build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static) -> ContextMenu<Self> {
        self.context_menu(built(build, Keys::Hidden))
    }

    /// Opens a menu with Copy, copying `text`, where this is pressed long under touch, for what a copy button shown
    /// under the pointer copies with one, since nothing rests on it under touch, as `docs/spec/components.md` says.
    /// With a pointer it's as it was, since its button copies it. A menu takes its trigger's element ID, so this has
    /// one of its own.
    fn copied_by_a_long_press(self, text: impl Into<SharedString>, cx: &App) -> AnyElement
    where
        Self: ParentElement + Styled + IntoElement + 'static,
    {
        if Input::of(cx) == Input::Pointer {
            return self.into_any_element();
        }
        let text = text.into();
        self.right_click_menu(move |menu, _, _| menu.row(copy_row(text.clone()))).into_any_element()
    }
}

/// The row that copies `text`.
fn copy_row(text: SharedString) -> MenuRow {
    menu_row("Copy")
        .icon(gpui_kit::assets::IconName::Copy)
        .copies()
        .on_choose(move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())))
}

impl<T: ContextMenuExt + Sized> RightClickMenu for T {}

impl gpui_kit::prelude::FluentBuilder for Menu {}

/// A menu `build` fills, drawn where it's put rather than opened from anything, for a sheet of how menus look.
#[cfg(any(test, feature = "testing"))]
pub fn shown_menu(
    window: &mut Window,
    cx: &mut App,
    build: impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static,
) -> gpui_kit::AnyView {
    PopupMenu::build(window, cx, menu(build)).into()
}
