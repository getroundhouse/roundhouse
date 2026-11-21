//! That a window selects what the client's copy of Ghostty selected for the same presses and drags.

use protocol::terminal::PictureSending;
use terminal_state::drawn::Selected;
use terminal_state::{Mirror, Pointer, SelectBy, Size};

use super::super::{Ask, Replied, Window};
use super::feeding::{watched, worker};

const SIZE: Size = Size { cols: 10, rows: 4 };
const OUTPUT: &str = "one two\r\nthree four\r\n日本 five\r\nsix";

/// Every place a pointer is tried at: either side of the six tenths of a cell, in cells along each row and the edges,
/// and past each edge.
fn places() -> Vec<Pointer> {
    let along = [-1.5, 0.2, 0.7, 1.5, 4.1, 4.65, 8.3, 9.2, 9.9, 11.0];
    let down = [-0.5, 0.5, 1.5, 2.5, 3.5, 4.5];
    down.iter().flat_map(|row| along.iter().map(|col| Pointer { col: *col, row: *row })).collect()
}

fn selections(rows: &[std::sync::Arc<terminal_state::drawn::Row>]) -> Vec<Option<Selected>> {
    rows.iter().map(|row| row.selected).collect()
}

#[test]
fn a_drag_by_cells_selects_what_ghosttys_selected() {
    let mut ghostty = Mirror::new(SIZE).unwrap();
    ghostty.write(OUTPUT.as_bytes());
    let mut worker = worker(SIZE, OUTPUT.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let places = places();

    for press in &places {
        for drag in &places {
            ghostty.press(*press, SelectBy::Cells).unwrap();
            ghostty.drag(*drag).unwrap();
            let ghosttys = selections(&ghostty.frame().unwrap().rows);
            assert!(window.press(*press, SelectBy::Cells).is_empty());
            assert!(window.drag(*drag).is_empty());
            let windows = window.frame().map(|frame| selections(&frame.rows)).unwrap();

            assert_eq!(windows, ghosttys, "a press at {press:?} dragged to {drag:?}");
            ghostty.release(*drag).unwrap();
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
    let selected = window.selected();

    window.press(Pointer { col: 3.2, row: 0.5 }, SelectBy::Cells);
    window.release();

    assert!(selected.is_some());
    assert_eq!(window.selected(), None);
    assert!(selections(&window.frame().unwrap().rows).iter().all(Option::is_none));
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
    let mut ghostty = Mirror::new(SIZE).unwrap();
    ghostty.write(OUTPUT.as_bytes());
    let mut worker = worker(SIZE, OUTPUT.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let on_words = [(0.5, 0.5), (5.5, 0.5), (1.5, 1.5), (7.5, 1.5), (0.5, 2.5), (6.5, 2.5), (1.5, 3.5)];
    let on_words: Vec<_> = on_words.iter().map(|(col, row)| Pointer { col: *col, row: *row }).collect();

    for by in [SelectBy::Words, SelectBy::Lines] {
        for press in &on_words {
            for drag in &on_words {
                ghostty.press(*press, by).unwrap();
                ghostty.drag(*drag).unwrap();
                let ghosttys = selections(&ghostty.frame().unwrap().rows);
                let asks = window.press(*press, by);
                answered(&mut window, &mut worker, asks);
                let asks = window.drag(*drag);
                answered(&mut window, &mut worker, asks);
                let windows = window.frame().map(|frame| selections(&frame.rows)).unwrap();

                assert_eq!(windows, ghosttys, "a press by {by:?} at {press:?} dragged to {drag:?}");
                ghostty.release(*drag).unwrap();
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
