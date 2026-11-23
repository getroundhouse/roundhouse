use super::{shortened, whole};

#[test]
fn a_whole_count_has_a_comma_between_each_three_figures_from_a_thousand() {
    let cases = [
        (0, "0"),
        (7, "7"),
        (999, "999"),
        (1_000, "1,000"),
        (1_212, "1,212"),
        (48_221, "48,221"),
        (999_999, "999,999"),
        (1_234_567, "1,234,567"),
        (u64::MAX, "18,446,744,073,709,551,615"),
    ];
    for (count, written) in cases {
        assert_eq!(whole(count), written, "{count}");
    }
}

/// A count of any width is written the same, so a `usize` of files reads as a `u64` the hub sent does.
#[test]
fn a_whole_count_is_written_the_same_whatever_its_width() {
    assert_eq!([whole(1_212_u16), whole(1_212_u32), whole(1_212_usize), whole(1_212_u128)], ["1,212"; 4]);
}

#[test]
fn a_shortened_count_is_whole_below_a_thousand_and_cut_to_a_tenth_below_ten_of_a_unit() {
    let cases = [
        (0, "0"),
        (999, "999"),
        (1_000, "1k"),
        (1_099, "1k"),
        (1_250, "1.2k"),
        (9_999, "9.9k"),
        (10_000, "10k"),
        (37_412, "37k"),
        (999_999, "999k"),
        (1_000_000, "1M"),
        (1_480_000, "1.4M"),
        (37_000_000, "37M"),
        (4_000_000_000, "4000M"),
    ];
    for (count, written) in cases {
        assert_eq!(shortened(count), written, "{count}");
    }
}
