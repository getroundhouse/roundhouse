use super::ControlSize;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_control_size_in_order_as_the_app_sizes_it() {
    let table: Vec<[String; 5]> = spec_table::rows("**Every size of a control, by its name.**");

    let sized: Vec<[String; 5]> = ControlSize::ALL.iter().map(|size| size.written()).collect();
    assert_eq!(table, sized, "docs/spec/look.md's table of a control's sizes, against crates/look/src/control_sizes");
}
