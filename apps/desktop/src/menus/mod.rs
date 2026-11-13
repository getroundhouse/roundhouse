//! The menus, and what their items do that no view does. Each item is an action, so the key bound to it shows beside
//! it. On a Mac they're macOS's menu bar. Elsewhere GPUI keeps them for the app to draw, which it does behind a button
//! in the window's top left corner, as `drawn` says, arranged as Windows arranges menus. Both come from here, so they
//! can't drift apart.

mod drawn;
#[cfg(test)]
mod tests;

pub(crate) use drawn::popup;

mod system_menu;
#[cfg(test)]
use system_menu::menus;
pub(crate) use system_menu::{Says, System, set, show};
