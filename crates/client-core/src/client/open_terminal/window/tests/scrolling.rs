//! That a window scrolls through the history as Ghostty did, fetching the rows it lacks near its view, and keeps its
//! place by row number as output comes.

use protocol::terminal::{MOST_ROWS, PictureSending, RowNumber, TerminalRowsReply, TerminalSize};
use terminal_state::Mirror;

use super::super::{Ask, Replied, Window};
use super::feeding::{pictured, watched, worker};

const SIZE: TerminalSize = TerminalSize { cols: 20, rows: 5 };

fn lines(count: usize) -> String {
    super::feeding::lines(0..count) + "$ "
}

/// Answers what a window asks for rows from `worker`, until it asks for no more, and how many asks there were.
fn fetched(window: &mut Window, worker: &mut Mirror, mut asks: Vec<Ask>) -> usize {
    let mut count = 0;
    while let Some(ask) = asks.pop() {
        let Ask::Rows { numbering, from, count: rows } = ask else { panic!("asked for {ask:?}") };
        assert!(rows <= MOST_ROWS);
        count += 1;
        let reply = worker.history_rows(numbering, from, rows).unwrap();
        asks.extend(window.replied(Replied::Rows(reply)));
    }
    count
}

fn text(window: &mut Window) -> Vec<String> {
    window.frame().unwrap().rows.iter().map(|row| row.text()).collect()
}

#[test]
fn the_wheel_scrolls_through_the_history_as_ghostty_did_once_the_rows_have_come() {
    let mut worker = worker(SIZE, lines(100).as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let from = |top: usize| (top..top + 5).map(|at| format!("line {at}")).collect::<Vec<_>>();
    let bottom = ["line 96", "line 97", "line 98", "line 99", "$"].map(String::from).to_vec();

    // What Ghostty showed after each turn, and where its cursor was, which is in sight only at the bottom, after the
    // prompt.
    for (lines, ghosttys, cursor) in [
        (-3, from(93), None),
        (-40, from(53), None),
        (7, from(60), None),
        (-200, from(0), None),
        (30, from(30), None),
        (400, bottom, Some((2, 4))),
    ] {
        let asks = window.wheel(lines);
        fetched(&mut window, &mut worker, asks);
        let frame = window.frame().unwrap();

        let windows: Vec<String> = frame.rows.iter().map(|row| row.text()).collect();
        assert_eq!(windows, ghosttys, "after turning the wheel by {lines}");
        assert_eq!(frame.cursor.map(|at| (at.col, at.row)), cursor, "after turning the wheel by {lines}");
    }
}

#[test]
fn rows_still_to_come_are_blank_and_those_in_sight_are_asked_for_first() {
    let mut worker = worker(SIZE, lines(100).as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());

    let asks = window.wheel(-50);

    assert_eq!(text(&mut window), ["", "", "", "", ""]);
    let [Ask::Rows { from, count, .. }] = asks.as_slice() else { panic!("asked for {asks:?}") };
    assert!(*from <= RowNumber::new(46) && from.plus(u64::from(*count)) > RowNumber::new(50), "{from:?} {count}");
    fetched(&mut window, &mut worker, asks);
    assert_eq!(text(&mut window), ["line 46", "line 47", "line 48", "line 49", "line 50"]);
}

/// Rows a reply brings in another numbering, after the terminal was resized meanwhile, aren't shown as these.
#[test]
fn rows_in_another_numbering_are_dropped() {
    let mut worker = worker(SIZE, lines(100).as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let asks = window.wheel(-50);
    let Ask::Rows { numbering, from, count } = asks[0] else { panic!("asked for {asks:?}") };
    let mut reply = worker.history_rows(numbering, from, count).unwrap();
    reply.numbering = numbering.next();

    window.replied(Replied::Rows(TerminalRowsReply { ..reply }));

    assert_eq!(text(&mut window), ["", "", "", "", ""]);
}

#[test]
fn output_while_scrolled_back_leaves_the_view_where_it_was() {
    let mut worker = worker(SIZE, lines(100).as_bytes());
    let mut sending = PictureSending::default();
    let mut window = watched(&mut worker, &mut sending);
    let asks = window.wheel(-10);
    fetched(&mut window, &mut worker, asks);
    let before = text(&mut window);

    worker.write(b"more\r\nand more\r\n$ ");
    for frame in pictured(&mut worker, &mut sending) {
        let asks = window.take(frame).unwrap();
        fetched(&mut window, &mut worker, asks);
    }

    assert_eq!(text(&mut window), before);
}

/// The rows that scroll off while the view is at the bottom are held, up to three screens of them, so a turn of the
/// wheel shows them at once.
#[test]
fn at_the_bottom_the_rows_that_last_scrolled_off_are_held() {
    let mut worker = worker(SIZE, b"");
    let mut sending = PictureSending::default();
    let mut window = watched(&mut worker, &mut sending);
    for at in 0..40 {
        worker.write(format!("line {at}\r\n").as_bytes());
        for frame in pictured(&mut worker, &mut sending) {
            assert!(window.take(frame).unwrap().is_empty(), "nothing is asked for at the bottom");
        }
    }

    let near = window.wheel(-15);
    let shown = text(&mut window);
    let far = window.wheel(-1);

    assert_eq!(shown, ["line 21", "line 22", "line 23", "line 24", "line 25"], "the three screens above are held");
    assert_eq!(near.len(), 1, "and the two screens above them are asked for");
    assert_eq!(text(&mut window)[0], "", "and the row above them shows once they come");
    assert!(far.is_empty(), "a window asks for one run of rows at a time");
}

/// As the view first shows a terminal, so a small scroll has them.
#[test]
fn the_two_screens_above_are_asked_for_as_a_terminal_is_first_shown() {
    let mut worker = worker(SIZE, lines(100).as_bytes());
    let mut window = Window::new();

    let asks: Vec<Ask> = pictured(&mut worker, &mut PictureSending::default())
        .into_iter()
        .flat_map(|frame| window.take(frame).unwrap())
        .collect();

    let [Ask::Rows { from, count, .. }] = asks.as_slice() else { panic!("asked for {asks:?}") };
    assert_eq!((from.get(), *count), (86, 10));
}
