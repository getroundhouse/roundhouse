use super::{ChipKind, chip};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_chip_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of chip, by its name.**");

    let kinds: Vec<[String; 1]> = ChipKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of chip, against crates/look/src/chips");
}

/// A file or a folder shows the last part of its path, and the rest show what they're named whole.
#[test]
fn a_chip_shows_a_paths_last_part() {
    assert_eq!(chip(ChipKind::File, "crates/protocol/src/model.rs").name(), "model.rs");
    assert_eq!(chip(ChipKind::Folder, "crates/protocol/").name(), "protocol");
    assert_eq!(chip(ChipKind::File, "README.md").name(), "README.md");
    assert_eq!(chip(ChipKind::Subagent, "Find every tooltip / in look").name(), "Find every tooltip / in look");
}

/// The lines a chip is about follow its name as an editor writes a place in a file.
#[test]
fn a_chips_lines_are_written_as_a_place_in_a_file() {
    let file = || chip(ChipKind::File, "model.rs");

    assert_eq!(file().lines(12, Some(40)).lines_written().as_deref(), Some(":12\u{2013}40"));
    assert_eq!(file().lines(12, Some(12)).lines_written().as_deref(), Some(":12"));
    assert_eq!(file().lines(12, None).lines_written().as_deref(), Some(":12\u{2013}"));
    assert_eq!(file().lines_written(), None);
}
