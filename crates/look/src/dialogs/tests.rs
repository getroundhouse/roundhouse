use super::DialogWidth;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_width_of_dialog_in_order() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every width of dialog, by its name.**");

    let widths: Vec<[String; 2]> = DialogWidth::ALL.iter().map(|width| width.written()).collect();
    assert_eq!(table, widths, "docs/spec/components.md's table of a dialog's widths, against crates/look/src/dialogs");
}
