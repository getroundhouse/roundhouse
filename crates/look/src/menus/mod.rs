//! Menus: what a button, a right-click or a menu bar opens, its rows as tall as a large control, with a card's corners
//! around them, as `docs/spec/components.md` says. The kit's menu draws them, from the style set here once for the
//! whole app, and [`Menu`] is the only way to fill one, so every menu's rows are one of a few kinds.

#[cfg(test)]
mod tests;

mod menu;
mod notes;
mod row;
mod style;

#[cfg(any(test, feature = "testing"))]
pub use menu::shown_menu;
pub use menu::{Menu, OpenMenu, RightClickMenu, menu};
pub use notes::NoteKind;
pub use row::{MenuRow, menu_row};
pub(crate) use style::style;
