//! The ends of a word or a line and the text of cells, as the worker's copy finds them by number, against what a
//! click in Ghostty selects and copies.

use libghostty_vt::selection::gesture::Behavior;
use protocol::terminal::{Bounds, Numbering, RowNumber, TerminalPoint, TerminalRange, TerminalSize, TerminalTextReply};

use crate::Mirror;
use crate::mirror::rows::counting::Numbered;

const SIZE: TerminalSize = TerminalSize { cols: 20, rows: 5 };

/// Lines a terminal wrapped, lines with spaces at their ends, a blank line, and a word cut by the wrap, in history and
/// on the screen.
const WRITTEN: &str = concat!(
    "first line\r\n",
    "a line that is longer than one row of twenty\r\n",
    "trailing   \r\n",
    "\r\n",
    "abcdefghijklmnopqrstuvwxyz0123456789 end\r\n",
    "x\r\n",
    "y y y y y y y y y y y y y y y\r\n",
    "last",
);

fn written() -> (Mirror, Numbered) {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(WRITTEN.as_bytes());
    let now = mirror.numbered().unwrap();
    (mirror, now)
}

fn point(row: RowNumber, col: u16) -> TerminalPoint {
    TerminalPoint { row, col }
}

/// Everything from the oldest row the copy keeps to the screen's last cell.
fn everything(now: Numbered) -> TerminalRange {
    TerminalRange { from: point(now.first, 0), to: point(now.top.plus(u64::from(SIZE.rows) - 1), SIZE.cols - 1) }
}

/// The pages joined as a client joins them.
fn copied(mirror: &mut Mirror, numbering: Numbering, range: TerminalRange, rectangle: bool, page: usize) -> String {
    let mut text = String::new();
    let mut from = range.from;
    loop {
        let TerminalTextReply { text: got, rest, joins, .. } =
            mirror.paged(numbering, TerminalRange { from, ..range }, rectangle, page).unwrap();
        text.push_str(&got);
        let Some(rest) = rest else { return text };
        if !joins {
            text.push('\n');
        }
        from = rest;
    }
}

/// However small a page is, the pages joined are what one copy of everything would be, which is what Select All
/// copies today, though a page ends at a line's end. A line the terminal wrapped is one line, as it was written.
#[test]
fn the_pages_joined_are_what_copying_everything_at_once_is() {
    let (mut mirror, now) = written();
    mirror.select_all();
    let all = mirror.selected_text().unwrap();
    mirror.clear_selection();

    assert!(all.contains("\na line that is longer than one row of twenty\n"), "{all:?}");
    for page in [1, 20, 30, 45, 64, 100, usize::MAX] {
        assert_eq!(copied(&mut mirror, now.numbering, everything(now), false, page), all, "pages of {page} bytes");
    }
}

/// A run of blank lines is copied as it is, however pages cut it, and those at the end are left out, as copying
/// everything at once leaves them out.
#[test]
fn blank_lines_are_copied_as_at_once_wherever_a_page_ends() {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(b"a\r\n\r\n\r\n\r\nb\r\n  \r\nc\r\n\r\n");
    let now = mirror.numbered().unwrap();
    mirror.select_all();
    let all = mirror.selected_text().unwrap();

    assert_eq!(all, "a\n\n\n\nb\n\nc");
    for page in [1, 20, 40, 60, usize::MAX] {
        assert_eq!(copied(&mut mirror, now.numbering, everything(now), false, page), all, "pages of {page} bytes");
    }
    // Only the blank rows at the end, which come to nothing however they're paged.
    let screen_end = point(now.top.plus(u64::from(SIZE.rows) - 1), SIZE.cols - 1);
    let blank_end = TerminalRange { from: point(RowNumber::new(screen_end.row.get() - 1), 0), to: screen_end };
    for page in [1, usize::MAX] {
        assert_eq!(copied(&mut mirror, now.numbering, blank_end, false, page), "", "pages of {page} bytes");
    }
    // To the spaces before a `c` written on the last row, which are kept, as what was written is.
    mirror.clear_selection();
    mirror.write(b"  c");
    let row = mirror.numbered().unwrap().top.plus(u64::from(SIZE.rows) - 1);
    let before_c = TerminalRange { to: point(row, 1), ..everything(now) };
    let at_once = copied(&mut mirror, now.numbering, before_c, false, usize::MAX);
    assert_eq!(at_once, "a\n\n\n\nb\n\nc\n\n");
    for page in [1, 20, 40] {
        assert_eq!(copied(&mut mirror, now.numbering, before_c, false, page), at_once, "pages of {page} bytes");
    }
}

/// A rectangle's columns, each row a line of its own, the same in pages as at once.
#[test]
fn a_rectangles_pages_joined_are_what_it_is_at_once() {
    let (mut mirror, now) = written();
    let range = TerminalRange { from: point(now.top, 2), to: point(now.top.plus(4), 6) };

    let at_once = copied(&mut mirror, now.numbering, range, true, usize::MAX);

    assert_eq!(at_once.lines().count(), 5, "{at_once:?}");
    for page in [1, 10, 25] {
        assert_eq!(copied(&mut mirror, now.numbering, range, true, page), at_once, "pages of {page} bytes");
    }
}

/// Where a double or a triple click selects is what's found, and what's copied of it is what the click copies, for a
/// word cut by the wrap, whose end is on a row the click wasn't on.
#[test]
fn a_words_and_a_lines_ends_are_where_a_click_selects() {
    let (mut mirror, now) = written();
    // The screen's rows, as it shows them with no scrolling: the long word's second half, then `x`, two of `y`, `last`.
    for (row, col, by, of) in [
        (0_u16, 3_u16, Behavior::Word, Bounds::Word),
        (2, 4, Behavior::Word, Bounds::Word),
        (2, 4, Behavior::Line, Bounds::Line),
        (0, 1, Behavior::Line, Bounds::Line),
    ] {
        mirror.click(col, row, by);
        let clicked = mirror.selected_text();
        mirror.clear_selection();
        let at = point(now.top.plus(u64::from(row)), col);

        let found = mirror.bounds(now.numbering, at, of).unwrap().range.unwrap();
        let text = copied(&mut mirror, now.numbering, found, false, usize::MAX);

        assert_eq!(Some(text), clicked, "{of:?} at {row}, {col}");
    }
    let word = mirror.bounds(now.numbering, point(now.top, 3), Bounds::Word).unwrap().range.unwrap();
    assert!(word.from.row < now.top, "the word began on the row before: {word:?}");
}

/// Nothing is found in another numbering, past what the terminal keeps, or for what a newer client asks the ends of,
/// and nothing is copied in another numbering.
#[test]
fn nothing_is_found_or_copied_where_the_terminal_has_nothing() {
    let (mut mirror, now) = written();
    let other = Numbering::new(now.numbering.get() + 1);
    let past = point(now.top.plus(u64::from(SIZE.rows)), 0);

    assert_eq!(mirror.bounds(other, point(now.top, 0), Bounds::Word).unwrap().range, None);
    assert_eq!(mirror.bounds(now.numbering, past, Bounds::Word).unwrap().range, None);
    assert_eq!(mirror.bounds(now.numbering, point(now.top, 0), Bounds::Unknown).unwrap().range, None);
    let copied = mirror.text(other, everything(now), false).unwrap();
    assert_eq!((copied.numbering, copied.text.as_str(), copied.rest), (now.numbering, "", None));
}

/// A full-screen program's screen, numbered apart from the shell's, has its own text, and its word's ends.
#[test]
fn a_programs_screen_has_its_own_text_and_words() {
    let (mut mirror, _) = written();
    mirror.write(b"\x1b[?1049h\x1b[Hprogram's own words");
    let now = mirror.numbered().unwrap();
    let screen = TerminalRange { from: point(now.top, 0), to: point(now.top.plus(4), SIZE.cols - 1) };

    let text = mirror.text(now.numbering, screen, false).unwrap().text;
    let word = mirror.bounds(now.numbering, point(now.top, 11), Bounds::Word).unwrap().range.unwrap();

    assert_eq!(text.trim_end(), "program's own words");
    assert_eq!((word.from.col, word.to.col), (10, 12), "{word:?}");
}
