use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use crate::{Mirror, Pointer, SelectBy, Size};

const SIZE: Size = Size { cols: 30, rows: 6 };

/// A numbered line, so no two rows are alike, with some long enough to wrap, and some with wide characters.
fn line(at: usize) -> String {
    let wide = if at.is_multiple_of(5) { "\u{4e2d}\u{6587} " } else { "" };
    let long = if at.is_multiple_of(7) { "y".repeat(40) } else { String::new() };
    format!("\x1b[3{}m{wide}line {at} {long}\x1b[0m\r\n", at % 8)
}

fn write_lines(mirror: &mut Mirror, lines: std::ops::Range<usize>) {
    for at in lines {
        mirror.write(line(at).as_bytes());
    }
}

/// A client's copy, started from what `worker` shows.
fn attached(worker: &Mirror, size: Size) -> Mirror {
    let mut copy = Mirror::new(size).unwrap();
    copy.write(&worker.snapshot().unwrap());
    copy
}

fn view(mirror: &mut Mirror) -> Vec<String> {
    mirror.frame().unwrap().rows.iter().map(|row| row.text().trim_end().to_owned()).collect()
}

/// What's selected, with no line breaks, since a new screen writes a line that wrapped as lines of its own, which copy
/// with a break between them (#2073).
fn copied(mirror: &Mirror) -> Option<String> {
    mirror.selected_text().unwrap().map(|text| text.replace('\n', ""))
}

/// Rows `from` to `to` of the view, whole, with the drag still under way.
fn select_rows(mirror: &mut Mirror, from: u16, to: u16) {
    mirror.press(Pointer { col: 0., row: f32::from(from) }, SelectBy::Cells).unwrap();
    mirror.drag(Pointer { col: f32::from(SIZE.cols) - 0.01, row: f32::from(to) }).unwrap();
}

/// What a client's copy shows and has selected, before and after a new screen.
struct Seen {
    before: (Vec<String>, Option<String>),
    after: (Vec<String>, Option<String>),
    /// What a new copy shows with no place kept, which is the bottom.
    bottom: Vec<String>,
}

/// A client's copy started once `before` lines are written, which takes in `streamed` more, then scrolls up `up` lines
/// and selects rows of the view, and misses `missed` more, which `missing` writes, before a new screen at `size`.
fn seen(
    (before, streamed, missed): (usize, usize, usize),
    up: isize,
    selected: Option<(u16, u16)>,
    size: Size,
    missing: impl FnOnce(&mut Mirror),
) -> (Seen, Mirror) {
    let mut worker = Mirror::new(SIZE).unwrap();
    write_lines(&mut worker, 0..before);
    let mut client = attached(&worker, SIZE);
    for copy in [&mut worker, &mut client] {
        write_lines(copy, before..before + streamed);
    }
    client.wheel(-up).unwrap();
    if let Some((from, to)) = selected {
        select_rows(&mut client, from, to);
    }
    write_lines(&mut worker, before + streamed..before + streamed + missed);
    missing(&mut worker);
    worker.resize(size).unwrap();

    let mut kept = attached(&worker, size);
    kept.keep_place_of(&client).unwrap();

    let before = (view(&mut client), copied(&client));
    let after = (view(&mut kept), copied(&kept));
    (Seen { before, after, bottom: view(&mut attached(&worker, size)) }, kept)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shows {
    /// The same view, and the same text selected.
    ThePlace,
    /// The bottom, with nothing selected, as a new copy does.
    TheBottom,
}

/// One way a new screen comes to a client's copy, and what the new copy shows.
struct Case {
    name: &'static str,
    /// Lines written before the old copy starts, then taken in by it, then missed by it.
    lines: (usize, usize, usize),
    /// How many lines the old copy is scrolled up.
    up: isize,
    /// The rows of the old copy's view selected.
    selected: Option<(u16, u16)>,
    /// The size the new screen comes at.
    size: Size,
    /// What else the old copy misses.
    missing: fn(&mut Mirror),
    shows: Shows,
}

/// With the view scrolled up and two rows of it selected, at the same size.
fn case(name: &'static str, lines: (usize, usize, usize), shows: Shows) -> Case {
    Case { name, lines, up: 40, selected: Some((2, 3)), size: SIZE, missing: |_| {}, shows }
}

#[test]
fn a_new_copy_keeps_the_place_while_it_has_its_rows() {
    let cases = [
        case("attached again, with nothing missed", (300, 0, 0), Shows::ThePlace),
        case("with output missed", (300, 0, 150), Shows::ThePlace),
        Case { selected: None, ..case("with nothing selected", (300, 0, 150), Shows::ThePlace) },
        case("once the history is full", (5000, 0, 400), Shows::ThePlace),
        case("once output taken in has filled it", (300, 5000, 400), Shows::ThePlace),
        Case { up: 0, ..case("at the bottom, with what's selected there", (300, 0, 150), Shows::ThePlace) },
        case("once its rows have gone from the history", (300, 0, 5000), Shows::TheBottom),
        Case {
            missing: |worker| worker.write(b"\x1b[3J\x1b[2J"),
            ..case("once the history is cleared", (300, 0, 0), Shows::TheBottom)
        },
        Case { size: Size { cols: 20, ..SIZE }, ..case("at another size", (300, 0, 0), Shows::TheBottom) },
    ];

    let mut wrong = Vec::new();
    for Case { name, lines, up, selected, size, missing, shows } in cases {
        let (seen, mut kept) = seen(lines, up, selected, size, missing);
        let expected = match shows {
            Shows::ThePlace if up == 0 => (seen.bottom.clone(), seen.before.1.clone()),
            Shows::ThePlace => seen.before.clone(),
            Shows::TheBottom => (seen.bottom.clone(), None),
        };
        if seen.after != expected {
            wrong.push(format!("{name}: {:?}, not {expected:?}", seen.after));
        }
        // The drag under way in the old copy doesn't go on in the new one.
        kept.drag(Pointer { col: 0., row: 0. }).unwrap();
        if copied(&kept) != seen.after.1 {
            wrong.push(format!("{name}: a drag after it changed what's selected"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

proptest! {
    #![proptest_config(Config {
        cases: 64,
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// However much the old copy took in, and the new one has that the old one missed, the new copy shows the old
    /// one's place or the bottom, and never another place. A copy attached again with nothing missed, to a view
    /// scrolled back, shows the place, since every few rows in view here have a number of their own.
    #[test]
    fn a_new_copy_shows_the_old_ones_place_or_the_bottom(
        before in 0..4000usize,
        streamed in prop_oneof![Just(0), 0..2000usize],
        missed in prop_oneof![Just(0), 0..2000usize],
        up in 0..300isize,
        selected in proptest::option::of((0..SIZE.rows, 0..SIZE.rows)),
    ) {
        let (seen, _) = seen((before, streamed, missed), up, selected, SIZE, |_| {});
        let ((view_before, selected_before), (view_after, selected_after)) = (&seen.before, &seen.after);

        prop_assert!(selected_after.is_none() || selected_after == selected_before, "{:?}", seen.after);
        // A row with no text before can have some now, as the row the shell's prompt was on does.
        let same_place = view_after.iter().zip(view_before).all(|(after, before)| before.is_empty() || after == before);
        prop_assert!(*view_after == seen.bottom || same_place, "{:?}", seen.after);
        if streamed == 0 && missed == 0 && *view_before != seen.bottom {
            prop_assert_eq!(&seen.after, &seen.before);
        }
    }
}
