//! That a window selects what Ghostty selected for the same presses and drags.

use std::sync::Arc;

use protocol::terminal::{PictureSending, TerminalSize};
use terminal_state::Mirror;

use super::super::{Ask, Pointer, Replied, SelectBy, Window};
use super::feeding::{watched, worker};
use crate::drawn::Row;

const SIZE: TerminalSize = TerminalSize { cols: 10, rows: 4 };
const OUTPUT: &str = "one two\r\nthree four\r\n日本 five\r\nsix";

/// What Ghostty selected of [`OUTPUT`] for each press at one of [`places`], a line each, dragged to each of them, in
/// that order, as [`selected`] writes it.
const BY_CELLS: &str = include_str!("drags_by_cells.txt");

/// What Ghostty selected of [`OUTPUT`] for a press on a word by words or by lines, dragged to each word, in the order
/// of [`ON_WORDS`].
const BY_WORDS_AND_LINES: &str = "\
Words 0.5,0.5: 0.0-0.2 0.0-0.6 0.0-1.4 0.0-1.9 0.0-2.3 0.0-2.8 0.0-3.2
Words 5.5,0.5: 0.0-0.6 0.4-0.6 0.4-1.4 0.4-1.9 0.4-2.3 0.4-2.8 0.4-3.2
Words 1.5,1.5: 0.0-1.4 0.4-1.4 1.0-1.4 1.0-1.9 1.0-2.3 1.0-2.8 1.0-3.2
Words 7.5,1.5: 0.0-1.9 0.4-1.9 1.0-1.9 1.6-1.9 1.6-2.3 1.6-2.8 1.6-3.2
Words 0.5,2.5: 0.0-2.3 0.4-2.3 1.0-2.3 1.6-2.3 2.0-2.3 2.0-2.8 2.0-3.2
Words 6.5,2.5: 0.0-2.8 0.4-2.8 1.0-2.8 1.6-2.8 2.0-2.8 2.5-2.8 2.5-3.2
Words 1.5,3.5: 0.0-3.2 0.4-3.2 1.0-3.2 1.6-3.2 2.0-3.2 2.5-3.2 3.0-3.2
Lines 0.5,0.5: 0.0-0.6 0.0-0.6 0.0-1.9 0.0-1.9 0.0-2.8 0.0-2.8 0.0-3.2
Lines 5.5,0.5: 0.0-0.6 0.0-0.6 0.0-1.9 0.0-1.9 0.0-2.8 0.0-2.8 0.0-3.2
Lines 1.5,1.5: 0.0-1.9 0.0-1.9 1.0-1.9 1.0-1.9 1.0-2.8 1.0-2.8 1.0-3.2
Lines 7.5,1.5: 0.0-1.9 0.0-1.9 1.0-1.9 1.0-1.9 1.0-2.8 1.0-2.8 1.0-3.2
Lines 0.5,2.5: 0.0-2.8 0.0-2.8 1.0-2.8 1.0-2.8 2.0-2.8 2.0-2.8 2.0-3.2
Lines 6.5,2.5: 0.0-2.8 0.0-2.8 1.0-2.8 1.0-2.8 2.0-2.8 2.0-2.8 2.0-3.2
Lines 1.5,3.5: 0.0-3.2 0.0-3.2 1.0-3.2 1.0-3.2 2.0-3.2 2.0-3.2 3.0-3.2
";

/// A place on each word of [`OUTPUT`].
const ON_WORDS: [Pointer; 7] = [
    Pointer { col: 0.5, row: 0.5 },
    Pointer { col: 5.5, row: 0.5 },
    Pointer { col: 1.5, row: 1.5 },
    Pointer { col: 7.5, row: 1.5 },
    Pointer { col: 0.5, row: 2.5 },
    Pointer { col: 6.5, row: 2.5 },
    Pointer { col: 1.5, row: 3.5 },
];

/// Every place a pointer is tried at: either side of the six tenths of a cell, in cells along each row and the edges,
/// and past each edge.
fn places() -> Vec<Pointer> {
    let along = [-1.5, 0.2, 0.7, 1.5, 4.1, 4.65, 8.3, 9.2, 9.9, 11.0];
    let down = [-0.5, 0.5, 1.5, 2.5, 3.5, 4.5];
    down.iter().flat_map(|row| along.iter().map(|col| Pointer { col: *col, row: *row })).collect()
}

/// What's selected in `rows`, from its first cell to its last, each as its row and column, such as `0.4-1.9`, or `-`
/// for nothing. What isn't one run of cells, which Ghostty never selected here, is written row by row, to fail.
fn selected(rows: &[Arc<Row>]) -> String {
    let runs: Vec<_> = rows.iter().enumerate().filter_map(|(at, row)| Some((at, row.selected?))).collect();
    let (Some((first, from)), Some((last, to))) = (runs.first(), runs.last()) else { return "-".into() };
    let one_run = runs.iter().enumerate().all(|(nth, (at, run))| {
        let starts = nth == 0 || run.col == 0;
        let ends = nth + 1 == runs.len() || run.col + run.cells == SIZE.cols;
        *at == first + nth && starts && ends
    });
    if one_run { format!("{first}.{}-{last}.{}", from.col, to.col + to.cells - 1) } else { format!("{runs:?}") }
}

/// The line of `recorded` for a press at `press`, after `prefix`.
fn recorded<'r>(recorded: &'r str, prefix: &str, press: Pointer) -> Vec<&'r str> {
    let heading = format!("{prefix}{},{}:", press.col, press.row);
    let line = recorded.lines().find(|line| line.starts_with(&heading));
    let line = line.unwrap_or_else(|| panic!("nothing recorded for {heading}"));
    line[heading.len()..].split_whitespace().collect()
}

#[test]
fn a_drag_by_cells_selects_what_ghosttys_selected() {
    let mut worker = worker(SIZE, OUTPUT.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let places = places();

    for press in &places {
        let ghosttys = recorded(BY_CELLS, "", *press);
        assert_eq!(ghosttys.len(), places.len());
        for (drag, ghosttys) in places.iter().zip(ghosttys) {
            assert!(window.press(*press, SelectBy::Cells).is_empty());
            assert!(window.drag(*drag).is_empty());
            let windows = window.frame().map(|frame| selected(&frame.rows)).unwrap();

            assert_eq!(windows, ghosttys, "a press at {press:?} dragged to {drag:?}");
            window.release();
        }
    }
}

#[test]
fn a_click_with_no_drag_clears_what_was_selected() {
    let mut worker = worker(SIZE, OUTPUT.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    window.press(Pointer { col: 0.2, row: 0.5 }, SelectBy::Cells);
    window.drag(Pointer { col: 5.2, row: 1.5 });
    window.release();
    let before = window.selected();

    window.press(Pointer { col: 3.2, row: 0.5 }, SelectBy::Cells);
    window.release();

    assert!(before.is_some());
    assert_eq!(window.selected(), None);
    assert_eq!(selected(&window.frame().unwrap().rows), "-");
}

/// Answers what a window asks for a word's or a line's ends from `worker`, as the hub would from the worker.
fn answered(window: &mut Window, worker: &mut Mirror, asks: Vec<Ask>) {
    for ask in asks {
        let Ask::Bounds { numbering, at, of, step } = ask else { panic!("asked for {ask:?}") };
        let reply = worker.bounds(numbering, at, of).unwrap();
        assert!(window.replied(Replied::Bounds { step, reply }).is_empty());
    }
}

/// On words and lines, since Ghostty's drag over the spaces between them finds the nearest word, where the worker
/// finds none, and the selection stays as it was.
#[test]
fn a_drag_by_words_or_lines_selects_what_ghosttys_selected() {
    let mut worker = worker(SIZE, OUTPUT.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());

    for by in [SelectBy::Words, SelectBy::Lines] {
        for press in &ON_WORDS {
            let ghosttys = recorded(BY_WORDS_AND_LINES, &format!("{by:?} "), *press);
            assert_eq!(ghosttys.len(), ON_WORDS.len());
            for (drag, ghosttys) in ON_WORDS.iter().zip(ghosttys) {
                let asks = window.press(*press, by);
                answered(&mut window, &mut worker, asks);
                let asks = window.drag(*drag);
                answered(&mut window, &mut worker, asks);
                let windows = window.frame().map(|frame| selected(&frame.rows)).unwrap();

                assert_eq!(windows, ghosttys, "a press by {by:?} at {press:?} dragged to {drag:?}");
                window.release();
            }
        }
    }
}

/// A drag's ends come a round trip after it, so one that a later drag has passed changes nothing.
#[test]
fn the_ends_for_a_drag_a_later_one_has_passed_are_dropped() {
    let mut worker = worker(SIZE, OUTPUT.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let asks = window.press(Pointer { col: 0.5, row: 0.5 }, SelectBy::Words);
    answered(&mut window, &mut worker, asks);
    let early = window.drag(Pointer { col: 7.5, row: 1.5 });
    let late = window.drag(Pointer { col: 5.5, row: 0.5 });
    answered(&mut window, &mut worker, late);
    let after_late = window.selected();

    answered(&mut window, &mut worker, early);

    assert_eq!(window.selected(), after_late, "\"one two\", not as far as \"four\"");
}

/// What's selected is still to come until the newest press or drag by words has its ends, so a copy waits for them, and
/// not once the ask has failed, so a copy doesn't wait for ends that won't come.
#[test]
fn what_is_selected_is_coming_until_the_newest_ends_come() {
    let mut worker = worker(SIZE, OUTPUT.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());

    let pressed = window.press(Pointer { col: 0.5, row: 0.5 }, SelectBy::Words);
    let while_pressed = window.selection_coming();
    let early = window.drag(Pointer { col: 7.5, row: 1.5 });
    let late = window.drag(Pointer { col: 5.5, row: 0.5 });
    answered(&mut window, &mut worker, pressed);
    answered(&mut window, &mut worker, early);
    let before_the_newest = window.selection_coming();
    answered(&mut window, &mut worker, late);
    let after_the_newest = window.selection_coming();
    let pressed_again = window.press(Pointer { col: 0.5, row: 2.5 }, SelectBy::Lines);
    assert!(!pressed_again.is_empty());
    assert!(window.replied(Replied::Failed).is_empty());

    assert_eq!((while_pressed, before_the_newest, after_the_newest), (true, true, false));
    assert!(!window.selection_coming(), "not after the ask failed");
}
