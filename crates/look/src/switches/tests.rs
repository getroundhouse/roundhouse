use super::SwitchKind;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_switch_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of switch, by its name.**");

    let kinds: Vec<[String; 1]> = SwitchKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of switch, against crates/look/src/switches");
}
