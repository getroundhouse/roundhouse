//! The menus: what each holds, the key beside each item, and what the items do. Hiding, minimizing and zooming happen
//! in macOS, which a headless window doesn't have, so for those the tests stop at the action. On a Mac the menus are
//! the menu bar, and elsewhere the app draws them, behind the button `drawn.rs` tests.

#![cfg(test)]

#[cfg(not(target_os = "macos"))]
mod drawn;
mod file_and_view;

mod bindings;

pub(crate) use bindings::{CHECK_FOR_UPDATES, QUIT, SETTINGS, choose, items};
use bindings::{action, enabled, key, keyed, with_a_repository};
