use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

fn heights(rows: &[Option<f32>]) -> Heights {
    let mut heights = Heights::default();
    heights.set_rows(rows.len());
    for (row, height) in rows.iter().enumerate() {
        if let Some(height) = height {
            heights.measure(row, px(*height));
        }
    }
    heights
}

/// Where each row starts, and where the last ends.
fn starts(heights: &Heights) -> Vec<Pixels> {
    (0..=heights.rows()).map(|row| heights.above(row)).collect()
}

#[test]
fn a_row_never_measured_counts_as_the_average_of_those_that_have_been() {
    let none = heights(&[None, None]);
    let some = heights(&[Some(10.), None, Some(30.), None]);

    assert_eq!(none.guess(), narrow(FIRST_GUESS));
    assert_eq!(some.guess(), px(20.));
    assert_eq!((some.height_of(0), some.height_of(1)), (px(10.), px(20.)));
    assert_eq!(some.total(), px(80.));
}

#[test]
fn rows_that_show_nothing_dont_make_the_rest_count_as_nothing() {
    let all = heights(&[Some(0.), Some(0.), None, None]);

    assert_eq!(all.guess(), narrow(LEAST_GUESS));
    assert_eq!(all.total(), narrow(LEAST_GUESS * 2.));
}

#[test]
fn each_row_starts_where_the_ones_before_it_end() {
    let all = heights(&[Some(10.), None, Some(30.), None, Some(0.), Some(50.)]);

    // The four measured come to 90, so the others count as 22.5.
    assert_eq!(starts(&all), [0., 10., 32.5, 62.5, 85., 85., 135.].map(px));
}

#[test]
fn a_place_down_the_session_is_in_the_row_that_covers_it() {
    let all = heights(&[Some(10.), None, Some(30.), None, Some(0.), Some(50.)]);

    assert_eq!(all.row_at(px(0.)), (0, px(0.)));
    assert_eq!(all.row_at(px(9.)), (0, px(9.)));
    assert_eq!(all.row_at(px(10.)), (1, px(0.)));
    assert_eq!(all.row_at(px(40.)), (2, px(7.5)));
    assert_eq!(all.row_at(px(85.)), (5, px(0.)), "a row of no height covers nothing");
    assert_eq!(all.row_at(px(134.)), (5, px(49.)));
    assert_eq!(all.row_at(px(500.)), (5, px(415.)), "past the end, it's the last row");
}

#[test]
fn measuring_a_row_again_moves_everything_after_it() {
    let mut all = heights(&[Some(10.), Some(20.), Some(30.)]);

    all.measure(1, px(50.));

    assert_eq!(starts(&all), [0., 10., 60., 90.].map(px));
    assert_eq!(all.row_at(px(59.)), (1, px(49.)));
}

#[test]
fn rows_that_come_and_go_at_the_end_leave_the_rest_as_measured() {
    let mut all = heights(&[Some(10.), Some(20.), Some(30.)]);

    all.set_rows(5);
    assert_eq!(all.total(), px(100.), "two more, at the average of 20");
    all.set_rows(2);

    assert_eq!(all.total(), px(30.));
    assert_eq!(all.row_at(px(25.)), (1, px(15.)));
}

/// What happens to a session's rows while it's open.
#[derive(Clone, Debug)]
enum Change {
    /// It comes to this many rows, as when the agent adds some or a prompt that was sent is taken back.
    Rows(usize),
    /// A row is measured. The row is taken modulo how many there are.
    Measured(usize, u16),
}

fn change() -> impl Strategy<Value = Change> {
    prop_oneof![
        (0usize..70).prop_map(Change::Rows),
        // Quarters of a pixel, and often none, as many rows show nothing.
        (0usize..70, prop_oneof![Just(0u16), 0u16..2000]).prop_map(|(row, quarters)| Change::Measured(row, quarters)),
    ]
}

/// The heights after `changes`, and the same kept as a plain list of each row's height.
fn after(changes: &[Change]) -> (Heights, Vec<Option<f32>>) {
    let (mut heights, mut plain) = (Heights::default(), Vec::new());
    for change in changes {
        match change {
            Change::Rows(rows) => {
                heights.set_rows(*rows);
                plain.resize(*rows, None);
            }
            Change::Measured(row, quarters) if !plain.is_empty() => {
                let (row, height) = (row % plain.len(), f32::from(*quarters) / 4.);
                heights.measure(row, px(height));
                plain[row] = Some(height);
            }
            Change::Measured(..) => {}
        }
    }
    (heights, plain)
}

/// How tall each row of a plain list counts as: its own height, or the average of the measured ones.
fn counted(plain: &[Option<f32>]) -> Vec<f64> {
    let measured: Vec<f64> = plain.iter().flatten().map(|height| f64::from(*height)).collect();
    let guess = if measured.is_empty() {
        FIRST_GUESS
    } else {
        (measured.iter().sum::<f64>() / count(measured.len())).max(LEAST_GUESS)
    };
    plain.iter().map(|height| height.map_or(guess, f64::from)).collect()
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// However the rows came, went and were measured, each starts where adding up the ones before it says.
    #[test]
    fn a_row_starts_where_the_rows_before_it_add_up_to(changes in vec(change(), 0..200)) {
        let (heights, plain) = after(&changes);
        let counted = counted(&plain);

        prop_assert_eq!(heights.rows(), plain.len());
        let mut above = 0.;
        for (row, height) in counted.iter().enumerate() {
            prop_assert!((heights.above(row).to_f64() - above).abs() < 0.01, "row {} starts at {}", row, above);
            prop_assert!((heights.height_of(row).to_f64() - height).abs() < 0.01, "row {} is {} tall", row, height);
            above += height;
        }
        prop_assert!((heights.total().to_f64() - above).abs() < 0.01);
    }

    /// A place found in the session is in the row that covers it: the place is where that row starts, and that far
    /// into it.
    #[test]
    fn a_place_is_in_the_row_it_is_found_in(changes in vec(change(), 1..200), down in 0f32..1.2) {
        let (heights, plain) = after(&changes);
        prop_assume!(!plain.is_empty());
        let down = heights.total() * down;

        let (row, offset) = heights.row_at(down);

        prop_assert!(row < plain.len());
        prop_assert!(offset >= px(0.));
        prop_assert!((heights.above(row) + offset - down).abs() < px(0.01), "row {} and {:?} into it", row, offset);
        // Unless it's past the end, which is in the last row.
        if down < heights.total() - px(0.01) {
            prop_assert!(offset < heights.height_of(row) + px(0.01), "{:?} into row {}, which is {:?}", offset, row, heights.height_of(row));
        }
    }
}
