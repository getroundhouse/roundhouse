//! Cells kept through a new size, which must still be the same text after the terminal rewraps its rows.

use protocol::terminal::{Numbering, RowNumber, TerminalPoint, TerminalRange, TerminalSize};

use crate::Mirror;

const SIZE: TerminalSize = TerminalSize { cols: 20, rows: 5 };

/// The character in a cell, as the copy copies it.
fn char_at(mirror: &mut Mirror, numbering: Numbering, at: TerminalPoint) -> String {
    mirror.text(numbering, TerminalRange { from: at, to: at }, false).unwrap().text
}

/// A cell in history, one on the screen, and one in the second row of a wrapped line, each still the same character
/// once the terminal is narrower, and once it's wider again, though each new size starts a new numbering.
#[test]
fn a_kept_cell_is_the_same_character_after_a_new_size() {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(b"history H here\r\n");
    mirror.write(&b"filler\r\n".repeat(6));
    mirror.write(b"0123456789abcdefghijKLMNO wrapped\r\nscreen S\r\n");
    let now = mirror.numbered().unwrap();
    let points = [
        TerminalPoint { row: now.first, col: 8 },
        TerminalPoint { row: now.top.plus(3), col: 7 },
        TerminalPoint { row: now.top.plus(2), col: 2 },
    ];
    let before: Vec<_> = points.iter().map(|at| char_at(&mut mirror, now.numbering, *at)).collect();
    assert_eq!(before, ["H", "S", "M"]);

    let mut numbering = now.numbering;
    let mut points = points.map(Some);
    for size in [TerminalSize { cols: 12, rows: 6 }, TerminalSize { cols: 40, rows: 4 }] {
        let tracked = mirror.track(numbering, &points.map(Option::unwrap)).unwrap();
        mirror.resize(size).unwrap();
        let fitted = mirror.tracked(&tracked).unwrap();

        assert_ne!(fitted.numbering, numbering, "a new size starts a new numbering");
        numbering = fitted.numbering;
        points = [fitted.kept[0], fitted.kept[1], fitted.kept[2]];
        let after: Vec<_> = points.iter().map(|at| char_at(&mut mirror, numbering, at.unwrap())).collect();
        assert_eq!(after, before, "at {size:?}");
    }
}

/// The same size again keeps the numbering and every cell where it was, so a client that sends it twice loses
/// nothing, and a cell in another numbering, or past what the terminal keeps, isn't kept.
#[test]
fn the_same_size_again_keeps_everything_where_it_was() {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(b"one\r\ntwo");
    let now = mirror.numbered().unwrap();
    let points =
        [TerminalPoint { row: now.top, col: 1 }, TerminalPoint { row: now.top.plus(u64::from(SIZE.rows)), col: 0 }];

    let tracked = mirror.track(now.numbering, &points).unwrap();
    mirror.resize(SIZE).unwrap();
    let fitted = mirror.tracked(&tracked).unwrap();
    let elsewhere = mirror.track(Numbering::new(now.numbering.get() + 1), &points[..1]).unwrap();

    assert_eq!((fitted.numbering, fitted.kept), (now.numbering, vec![Some(points[0]), None]));
    assert_eq!(mirror.tracked(&elsewhere).unwrap().kept, [None]);
    assert_eq!(points[0].row, RowNumber::new(now.top.get()));
}
