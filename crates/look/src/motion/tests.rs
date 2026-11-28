use std::time::Duration;

use super::{FADE, PANEL, SCROLL};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_motion_in_order_as_long_as_the_app_takes() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every motion, by its name.**");

    let taken = |name: &str, time: Duration| [name.to_string(), format!("{}ms", time.as_millis())];
    let named = vec![taken("PANEL", PANEL), taken("FADE", FADE), taken("SCROLL", SCROLL)];
    assert_eq!(table, named, "docs/spec/look.md's table of motion, against crates/look/src/motion/transitions.rs");
}
