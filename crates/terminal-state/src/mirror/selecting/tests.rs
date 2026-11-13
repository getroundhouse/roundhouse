use crate::drawn::Selected;
use crate::{Mirror, Pointer, SelectBy, Size};

const SIZE: Size = Size { cols: 20, rows: 4 };

fn mirror_of(written: &[u8]) -> Mirror {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(written);
    mirror
}

fn at(col: f32, row: f32) -> Pointer {
    Pointer { col, row }
}

/// Pressed at the start of a cell and dragged into the second half of another, as a hand selects a word.
fn select(mirror: &mut Mirror, from: Pointer, to: Pointer) {
    mirror.press(from, SelectBy::Cells).unwrap();
    mirror.drag(to).unwrap();
    mirror.release(to).unwrap();
}

#[test]
fn a_drag_selects_from_cell_to_cell_which_shows_and_copies() {
    let mut mirror = mirror_of(b"hello world");
    mirror.frame().unwrap();

    select(&mut mirror, at(0.1, 0.5), at(4.9, 0.5));

    assert_eq!(mirror.selected_text().unwrap().as_deref(), Some("hello"));
    let frame = mirror.frame().unwrap();
    assert_eq!(frame.rows[0].selected, Some(Selected { col: 0, cells: 5 }), "a row that changed only by that");
    assert_eq!(frame.rows[1].selected, None);
}

#[test]
fn a_drag_over_lines_selects_to_the_end_of_each_and_copies_them_as_lines() {
    let mut mirror = mirror_of(b"one   \r\ntwo\r\nthree");

    select(&mut mirror, at(1.1, 0.5), at(1.9, 2.5));

    assert_eq!(mirror.selected_text().unwrap().as_deref(), Some("ne\ntwo\nth"), "without the spaces at the ends");
    let frame = mirror.frame().unwrap();
    assert_eq!(frame.rows[0].selected, Some(Selected { col: 1, cells: SIZE.cols - 1 }));
    assert_eq!(frame.rows[1].selected, Some(Selected { col: 0, cells: SIZE.cols }));
    assert_eq!(frame.rows[2].selected, Some(Selected { col: 0, cells: 2 }));
}

/// Ghostty moves a selection's start off a wide character's second cell, but leaves its end on the first, so the frame
/// has to take in the second too, or the character would show half selected though it's copied whole.
#[test]
fn a_selection_that_ends_on_a_wide_character_shows_all_of_it() {
    let mut mirror = mirror_of("日本".as_bytes());

    select(&mut mirror, at(0.1, 0.5), at(3.3, 0.5));

    assert_eq!(mirror.selected_text().unwrap().as_deref(), Some("日本"));
    assert_eq!(mirror.frame().unwrap().rows[0].selected, Some(Selected { col: 0, cells: 4 }));
}

/// As a hand drags down past the last line, which takes in everything to the end of the screen.
#[test]
fn a_drag_past_the_edges_selects_to_the_nearest_cell() {
    let mut mirror = mirror_of(b"one\r\ntwo");

    select(&mut mirror, at(1.1, 0.5), at(f32::from(SIZE.cols) + 5., f32::from(SIZE.rows) + 3.));

    assert_eq!(mirror.selected_text().unwrap().as_deref(), Some("ne\ntwo"));
    let last = usize::from(SIZE.rows) - 1;
    assert_eq!(mirror.frame().unwrap().rows[last].selected, Some(Selected { col: 0, cells: SIZE.cols }));
}

/// A click with no drag selects nothing, as in terminal apps, so it doesn't leave a cell selected.
#[test]
fn a_press_clears_what_was_selected() {
    let mut mirror = mirror_of(b"hello world");
    select(&mut mirror, at(0.1, 0.5), at(4.9, 0.5));
    mirror.frame().unwrap();

    mirror.press(at(8.5, 0.5), SelectBy::Cells).unwrap();
    mirror.release(at(8.5, 0.5)).unwrap();

    assert_eq!(mirror.selected_text().unwrap(), None);
    assert_eq!(mirror.frame().unwrap().rows[0].selected, None, "the row is drawn again without it");
}

#[test]
fn a_second_press_selects_the_word_and_a_third_the_line() {
    let mut mirror = mirror_of(b"git status --short");
    let on_status = at(6.5, 0.5);

    mirror.press(on_status, SelectBy::Cells).unwrap();
    mirror.release(on_status).unwrap();
    mirror.press(on_status, SelectBy::Words).unwrap();
    mirror.release(on_status).unwrap();
    let word = mirror.selected_text().unwrap();
    mirror.press(on_status, SelectBy::Lines).unwrap();
    mirror.release(on_status).unwrap();

    assert_eq!(word.as_deref(), Some("status"));
    assert_eq!(mirror.selected_text().unwrap().as_deref(), Some("git status --short"));
}

#[test]
fn a_drag_after_a_double_click_goes_on_by_words() {
    let mut mirror = mirror_of(b"git status --short");
    let on_git = at(1.5, 0.5);
    mirror.press(on_git, SelectBy::Cells).unwrap();
    mirror.release(on_git).unwrap();

    mirror.press(on_git, SelectBy::Words).unwrap();
    mirror.drag(at(13.5, 0.5)).unwrap();
    mirror.release(at(13.5, 0.5)).unwrap();

    assert_eq!(mirror.selected_text().unwrap().as_deref(), Some("git status --short"));
}

/// The platform says a press is a first click, so one soon after a double one starts again from its cell, whatever
/// Ghostty would have made of the time between them.
#[test]
fn a_first_click_after_a_double_one_selects_nothing() {
    let mut mirror = mirror_of(b"git status --short");
    mirror.press(at(6.5, 0.5), SelectBy::Cells).unwrap();
    mirror.press(at(6.5, 0.5), SelectBy::Words).unwrap();

    mirror.press(at(6.5, 0.5), SelectBy::Cells).unwrap();

    assert_eq!(mirror.selected_text().unwrap(), None);
}

/// Ghostty keeps the selection on the text it was made on as the program writes on and the screen scrolls.
#[test]
fn what_was_selected_stays_selected_as_the_screen_scrolls() {
    let mut mirror = mirror_of(b"first\r\n");
    select(&mut mirror, at(0.1, 0.5), at(4.9, 0.5));

    mirror.write(b"2\r\n3\r\n4\r\n5\r\n6\r\n");

    assert_eq!(mirror.selected_text().unwrap().as_deref(), Some("first"));
}

#[test]
fn select_all_takes_in_the_history() {
    let mut mirror = mirror_of(b"first\r\n2\r\n3\r\n4\r\n5\r\nlast");

    mirror.select_all().unwrap();

    let all = mirror.selected_text().unwrap().unwrap();
    assert!(all.starts_with("first\n") && all.ends_with("last"), "{all:?}");
}

#[test]
fn clearing_the_selection_leaves_nothing_selected() {
    let mut mirror = mirror_of(b"hello");
    mirror.select_all().unwrap();

    mirror.clear_selection().unwrap();

    assert_eq!(mirror.selected_text().unwrap(), None);
}
