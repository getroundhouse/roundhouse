use super::TabWidth;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_width_of_tab_in_order() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every width of a tab, by its name.**");

    let widths: Vec<[String; 2]> = TabWidth::ALL.iter().map(|width| width.written()).collect();
    assert_eq!(table, widths, "docs/spec/components.md's table of a tab's widths, against crates/look/src/tabs");
}
