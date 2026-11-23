use written_counts::shortened;

use super::{CountKind, LinesKind, count, lines_changed};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_count_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of count, by its name.**");

    let kinds: Vec<[String; 1]> = CountKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of count, against crates/look/src/counts");
}

#[test]
fn the_specs_table_has_every_kind_of_lines_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of lines added and removed, by its name.**");

    let kinds: Vec<[String; 1]> = LinesKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of lines, against crates/look/src/counts");
}

/// A minus sign, not a hyphen, and nothing for what's none, whatever the kind.
#[test]
fn lines_changed_leave_out_what_is_none() {
    for kind in LinesKind::ALL.iter().copied() {
        assert_eq!(lines_changed(kind, 12, 3).written(), (Some("+12".into()), Some("\u{2212}3".into())), "{kind:?}");
        assert_eq!(lines_changed(kind, 40, 0).written(), (Some("+40".into()), None), "{kind:?}");
        assert_eq!(lines_changed(kind, 0, 120).written(), (None, Some("\u{2212}120".into())), "{kind:?}");
        assert_eq!(lines_changed(kind, 0, 0).written(), (None, None), "{kind:?}");
    }
}

/// Whole, with a comma between each three figures from a thousand, whatever the kind, as `1,212` files.
#[test]
fn a_count_is_written_whole() {
    for kind in CountKind::ALL.iter().copied() {
        let written = [999, 1_000, 1_212].map(|how_many| count(kind, how_many).written());

        assert_eq!(written, ["999", "1,000", "1,212"], "{kind:?}");
    }
}

/// Whole, as a count is, unless they're asked to be shortened, as a workspace's are, which stay as they were.
#[test]
fn lines_changed_are_written_whole_unless_shortened() {
    for kind in LinesKind::ALL.iter().copied() {
        let whole = lines_changed(kind, 48_221, 999).written();
        let shortened = lines_changed(kind, 37_412, 1_000).written_as(shortened).written();

        assert_eq!(whole, (Some("+48,221".into()), Some("\u{2212}999".into())), "{kind:?}");
        assert_eq!(shortened, (Some("+37k".into()), Some("\u{2212}1k".into())), "{kind:?}");
    }
}
