use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::{Colors, hsla};
use crate::spec_table;
use crate::theme::hex;

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(17), failure_persistence: None, ..Config::default() })]

    /// So the spec's table and the kit's theme are given the colour each role is.
    #[test]
    fn any_colour_is_written_as_the_bytes_it_was_made_from(value: u32) {
        let written = hex(hsla(value));

        let digits = written.trim_start_matches('#');
        let read = u32::from_str_radix(digits, 16).unwrap();
        let read = if digits.len() == 6 { read << 8 | 0xff } else { read };
        prop_assert_eq!(read, value, "written {}", written);
    }
}

#[test]
fn the_specs_table_has_every_colour_in_its_group_in_order_as_the_app_draws_it() {
    let mut group = String::new();
    let mut table = Vec::new();
    // A group's row has its title in bold, and the colours under it are in it.
    for [name, value] in spec_table::rows("**Every colour, by its name.**") {
        match name.strip_prefix("**").and_then(|rest| rest.strip_suffix("**")) {
            Some(title) => title.clone_into(&mut group),
            None => table.push((group.clone(), name, value)),
        }
    }

    let drawn: Vec<(String, String, String)> = Colors::dark()
        .roles()
        .into_iter()
        .map(|role| (role.group.to_string(), role.name.to_string(), role.hex()))
        .collect();
    assert_eq!(table, drawn, "docs/spec/look.md's table of colours, against crates/look/src/colors.rs");
}
