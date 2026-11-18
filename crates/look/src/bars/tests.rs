use super::BarKind;
use crate::space::Space;
use crate::spec_table;
use crate::tabs::TAB_START;

#[test]
fn the_specs_table_has_every_kind_of_bar_in_order() {
    let table: Vec<[String; 3]> = spec_table::rows("**Every kind of bar, by its name.**");

    let kinds: Vec<[String; 3]> = BarKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of bar, against crates/look/src/bars");
}

/// A tab's name starts where a bar's words do, so the bars over and under a row of tabs line up with its names.
#[test]
fn a_tabs_name_starts_where_a_bars_words_do() {
    let words = BarKind::Words.start().size();
    let tabs = BarKind::Tabs.start().size() + TAB_START.size();

    assert_eq!(tabs, words);
    assert_eq!(BarKind::Words.start(), Space::Large);
}
