use super::{ALL, written};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_motion_in_order_as_long_as_the_app_takes() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every motion, by its name.**");

    assert_eq!(
        table,
        ALL.map(written),
        "docs/spec/look.md's table of motion, against crates/look/src/motion/transitions.rs"
    );
}
