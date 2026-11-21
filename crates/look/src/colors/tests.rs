use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::contrast::Contrast;
use super::oklab::Oklab;
use super::palette::DARK;
use super::ramp::{Lightness, Stop};
use super::srgb::Srgb;
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

    /// So a colour planned in Oklab is the one drawn, give or take the byte it's rounded to.
    #[test]
    fn any_colour_comes_back_from_oklab_as_itself(channels: [u8; 3]) {
        let color = Srgb(channels);

        prop_assert_eq!(Oklab::from(color).srgb(), color);
    }

    #[test]
    fn a_wash_at_none_is_what_it_is_over_and_at_all_is_itself(over: [u8; 3], under: [u8; 3]) {
        let (over, under) = (Srgb(over), Srgb(under));

        prop_assert_eq!(over.over(under, 0.0), under);
        prop_assert_eq!(over.over(under, 1.0), over);
    }

    /// So text asked for at a contrast has it, and isn't brighter than it needs to be.
    #[test]
    fn text_reaches_the_contrast_it_is_made_for_and_little_more(wanted in 15.0..100.0_f64) {
        let page = DARK.greys.grey(DARK.page);

        let text = DARK.greys.text(Contrast(wanted), page);

        let Contrast(has) = Contrast::of(text, page);
        prop_assert!(has >= wanted, "Lc {has} for {wanted}");
        prop_assert!(has < wanted + 2.0, "Lc {has} for {wanted}");
    }
}

/// As CSS Color 4 gives sRGB's red, green and blue in Oklab, and white, which is all lightness.
#[test]
fn oklab_is_as_css_gives_it() {
    let given = [
        (0xff_0000, [0.627_96, 0.224_86, 0.125_85]),
        (0x00_ff00, [0.866_44, -0.233_89, 0.179_50]),
        (0x00_00ff, [0.452_01, -0.032_46, -0.311_53]),
        (0xff_ffff, [1.0, 0.0, 0.0]),
    ];

    for (hex, [l, a, b]) in given {
        let made = Oklab::from(Srgb::from_hex(hex));
        let close = |made: f64, given: f64| (made - given).abs() < 0.000_01;
        assert!(close(made.l, l) && close(made.a, a) && close(made.b, b), "{hex:06x}: {made:?}");
    }
}

/// As APCA's own tests of 0.0.98G-4g give it, dark on light and light on dark.
#[test]
fn contrast_is_as_apca_gives_it() {
    let given = [
        (0x88_8888, 0xff_ffff, 63.056_469_930_209_424),
        (0xff_ffff, 0x88_8888, 68.541_464_366_449_62),
        (0x00_0000, 0xaa_aaaa, 58.146_262_578_561_334),
        (0xaa_aaaa, 0x00_0000, 56.241_133_368_397_42),
        (0x11_2233, 0xdd_eeff, 91.668_308_114_816_31),
        (0xdd_eeff, 0x11_2233, 93.067_700_494_842_75),
    ];

    for (text, page, lc) in given {
        let Contrast(made) = Contrast::of(Srgb::from_hex(text), Srgb::from_hex(page));
        assert!((made - lc).abs() < 1e-9, "#{text:06x} on #{page:06x}: {made}");
    }
}

/// So each step between greys looks as far as any other: every stop is within a fifth of a step of the lightness it's
/// planned at, once it's rounded to bytes.
#[test]
fn the_greys_are_evenly_spaced_in_lightness() {
    let ramp = DARK.greys;
    let step = (ramp.lightest.0 - ramp.darkest.0) / f64::from(ramp.stops - 1);

    for stop in 0..ramp.stops {
        let Lightness(planned) = Lightness(ramp.darkest.0 + f64::from(stop) * step);
        let made = Oklab::from(ramp.grey(Stop(stop))).l;
        assert!((made - planned).abs() < step / 5.0, "stop {stop}: {made} for {planned}");
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
    assert_eq!(table, drawn, "docs/spec/look.md's table of colours, against crates/look/src/colors/mod.rs");
}
