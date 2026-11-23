use super::{CountKind, LinesKind, lines_changed};
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

/// Each count as it's asked to be written, after its sign.
#[test]
fn lines_changed_write_each_count_as_asked() {
    let tens = |count: usize| format!("{}0s", count / 10);

    let written = lines_changed(LinesKind::Workspace, 37_412, 12).written_as(tens).written();

    assert_eq!(written, (Some("+37410s".into()), Some("\u{2212}10s".into())));
}
