use gpui_kit::OperatingSystem;

use super::{Key, KeyKind};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_key_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of key, by its name.**");

    let kinds: Vec<[String; 1]> = KeyKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of key, against crates/look/src/keys/mod.rs");
}

/// A key is written as the system it's drawn on writes it: Cmd on a Mac and Ctrl elsewhere, for `secondary`.
#[test]
fn a_key_is_written_as_this_system_writes_it() {
    let (os, says) = if cfg!(target_os = "macos") {
        (OperatingSystem::Mac, vec!["⌘", "F"])
    } else {
        (OperatingSystem::Linux, vec!["Ctrl+F"])
    };

    assert_eq!(Key::parts("secondary-f", os), says);
}

/// On a Mac, each held key is a part of its own, and the key pressed is one, however many letters it's written in.
#[test]
fn a_key_pressed_is_one_part_after_each_held() {
    if !cfg!(target_os = "macos") {
        return;
    }
    assert_eq!(Key::parts("shift-tab", OperatingSystem::Mac), ["⇧", "Tab"]);
    assert_eq!(Key::parts("cmd-shift-p", OperatingSystem::Mac), ["⇧", "⌘", "P"]);
    assert_eq!(Key::parts("enter", OperatingSystem::Mac), ["⏎"]);
}

/// Escape is written in words everywhere, since the Mac's symbol for it reads as undo.
#[test]
fn escape_is_written_esc() {
    let os = if cfg!(target_os = "macos") { OperatingSystem::Mac } else { OperatingSystem::Linux };
    assert_eq!(Key::parts("escape", os), ["Esc"]);
    if cfg!(target_os = "macos") {
        assert_eq!(Key::parts("shift-escape", os), ["⇧", "Esc"]);
    }
}

#[test]
fn what_isnt_a_key_has_no_parts() {
    assert!(Key::parts("not-a-key-at-all-", OperatingSystem::Mac).is_empty());
}
