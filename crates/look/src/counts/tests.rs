use super::{CountKind, lines_changed};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_count_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of count, by its name.**");

    let kinds: Vec<[String; 1]> = CountKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of count, against crates/look/src/counts");
}

/// A minus sign, not a hyphen, and nothing for what's none.
#[test]
fn lines_changed_leave_out_what_is_none() {
    assert_eq!(lines_changed(12, 3).written(), (Some("+12".into()), Some("\u{2212}3".into())));
    assert_eq!(lines_changed(40, 0).written(), (Some("+40".into()), None));
    assert_eq!(lines_changed(0, 120).written(), (None, Some("\u{2212}120".into())));
}
