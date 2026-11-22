//! That a window scrolls through the history as Ghostty did, fetching the rows it lacks near its view, and keeps its
//! place by row number as output comes.

use std::collections::HashMap;
use std::ops::Range;

use protocol::terminal::{
    CellStyle, MOST_ROWS, PictureSending, RowNumber, TerminalRow, TerminalRowsReply, TerminalSize,
};
use terminal_state::Mirror;

use super::super::{Ask, Replied, Window};
use super::feeding::{pictured, watched, worker};

const SIZE: TerminalSize = TerminalSize { cols: 20, rows: 5 };

fn lines(count: usize) -> String {
    super::feeding::lines(0..count) + "$ "
}

/// Answers what a window asks for rows from `worker`, until it asks for no more, and the rows each ask was for. It fails
/// a window that goes on asking long after it could have asked for the whole history.
fn fetched(window: &mut Window, worker: &mut Mirror, asks: Vec<Ask>) -> Vec<Range<u64>> {
    fetched_as(window, worker, asks, |_| {})
}

/// As [`fetched`], with each reply's rows changed by `change` first.
fn fetched_as(
    window: &mut Window,
    worker: &mut Mirror,
    mut asks: Vec<Ask>,
    change: impl Fn(&mut TerminalRowsReply),
) -> Vec<Range<u64>> {
    let mut asked = Vec::new();
    while let Some(ask) = asks.pop() {
        asked.push(rows(&ask));
        assert!(asked.len() < 1_000, "it went on asking, for {:?}", &asked[asked.len() - 10..]);
        let Ask::Rows { numbering, from, count } = ask else { panic!("asked for {ask:?}") };
        assert!(count <= MOST_ROWS);
        let mut reply = worker.history_rows(numbering, from, count).unwrap();
        change(&mut reply);
        asks.extend(window.replied(Replied::Rows { numbering, from, reply }));
    }
    asked
}

/// The rows an ask is for.
fn rows(ask: &Ask) -> Range<u64> {
    let Ask::Rows { from, count, .. } = ask else { panic!("asked for {ask:?}") };
    from.get()..from.plus(u64::from(*count)).get()
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
    let Some(Ask::Rows { from, count, .. }) = asks.first() else { panic!("asked for {asks:?}") };
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

    window.replied(Replied::Rows { numbering, from, reply });

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
    assert_eq!(near.len(), 1, "and the rows above them are asked for");
    assert_eq!(text(&mut window)[0], "", "and the row above them shows once they come");
    assert!(far.is_empty(), "and rows asked for aren't asked for again before they come");
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

/// Scrolled back, a window asks for the rows in sight, two screens behind the view and 600 ahead of it, in the way it
/// last moved, four asks at once, nearest first, so a fling finds its rows there.
#[test]
fn a_scroll_asks_ahead_in_the_way_it_moved_several_runs_at_once() {
    let mut worker = worker(SIZE, lines(3_000).as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());

    let up = window.wheel(-1_000);
    let more = window.wheel(-1);
    let answered = fetched(&mut window, &mut worker, up.clone());
    let down = window.wheel(300);

    let runs = |asks: &[Ask]| asks.iter().map(rows).collect::<Vec<_>>();
    // From line 1996, the lines in sight and two screens below, then 600 above, a reply's worth at a time.
    assert_eq!(runs(&up), [1996..2011, 1796..1996, 1596..1796, 1396..1596]);
    assert!(more.is_empty(), "no more than four asks wait at once: {more:?}");
    assert!(answered.contains(&(1395..1396)), "and once they come, the row the turn after them lacks is asked for");
    // From line 2295, the lines in sight and below, then two screens above, then on below to 600 ahead.
    assert_eq!(runs(&down), [2295..2495, 2285..2295, 2495..2695, 2695..2895]);
}

/// A window lets go of the rows furthest from what it wants first, never the lead ahead of the view, even when the lead
/// and what's behind it come near the most rows it holds, so scrolling one way asks for no row twice.
#[test]
fn letting_go_of_rows_keeps_the_lead_so_scrolling_one_way_asks_for_no_row_twice() {
    // A lead of 1,000 rows, the most it can be, with 200 behind it and 100 in sight.
    let size = TerminalSize { cols: 20, rows: 100 };
    let mut worker = worker(size, lines(5_000).as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());

    let mut times_asked = HashMap::<u64, usize>::new();
    for _ in 0..50 {
        let asks = window.wheel(-100);
        for row in fetched(&mut window, &mut worker, asks).into_iter().flatten() {
            *times_asked.entry(row).or_default() += 1;
        }
    }

    assert!(times_asked.len() > 4_000, "the history was asked for: {}", times_asked.len());
    assert_eq!(asked_twice(times_asked), 0, "rows asked for again");
}

/// Rows so large that 2,000 would take more than the most bytes a window holds shorten its lead to what it has room for,
/// once it holds some, so it lets go of none of the rows it wants, scrolling or held still. Those it asked for before
/// it knew their size can be let go of, and asked for again.
#[test]
fn rows_too_large_to_hold_two_thousand_of_shorten_the_lead_so_none_is_asked_for_twice() {
    let mut worker = worker(SIZE, lines(3_000).as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    // About 18 KB a row as JSON, as a row 600 columns wide with a colour in each cell is.
    let large = |reply: &mut TerminalRowsReply| {
        let row = TerminalRow { styles: vec![CellStyle::default(); 6_000], ..TerminalRow::default() };
        reply.rows.fill(row);
    };
    let asks = window.wheel(-50);
    fetched_as(&mut window, &mut worker, asks, large);

    let mut times_asked = HashMap::<u64, usize>::new();
    for _ in 0..40 {
        let asks = window.wheel(-50);
        for row in fetched_as(&mut window, &mut worker, asks, large).into_iter().flatten() {
            *times_asked.entry(row).or_default() += 1;
        }
    }
    let held_still: Vec<_> = (0..3).flat_map(|_| window.wheel(0)).collect();

    assert!(times_asked.len() > 1_500, "the history was asked for: {}", times_asked.len());
    assert_eq!(asked_twice(times_asked), 0, "rows asked for again");
    assert!(held_still.is_empty(), "held still, it asked for {held_still:?}");
}

/// A window that wants more rows than it has room for, as one 200 rows tall does with rows that large, holds what it
/// can nearest the view, and stops asking until something changes, rather than letting go of rows and asking again.
#[test]
fn a_window_that_wants_more_rows_than_it_has_room_for_stops_asking() {
    let size = TerminalSize { cols: 20, rows: 200 };
    let mut worker = worker(size, lines(3_000).as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let large = |reply: &mut TerminalRowsReply| {
        let row = TerminalRow { styles: vec![CellStyle::default(); 6_000], ..TerminalRow::default() };
        reply.rows.fill(row);
    };

    let asks = window.wheel(-1_000);
    fetched_as(&mut window, &mut worker, asks, large);

    let top = window.scrolled.unwrap();
    assert!((0..200).all(|at| window.rows.has(top.plus(at))), "every row in sight is held");
}

/// How many rows were asked for more than once.
fn asked_twice(times_asked: HashMap<u64, usize>) -> usize {
    times_asked.into_values().filter(|times| *times > 1).count()
}
