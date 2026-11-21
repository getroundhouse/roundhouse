//! That a window keeps its place and what's selected through a new size it asked for, and goes to the bottom with
//! nothing selected after one another device gave the terminal.

use protocol::terminal::{PictureSending, TerminalPicture, Typed};
use terminal_state::{Mirror, Pointer, SelectBy, Size};

use super::super::{Ask, Replied, Window};
use super::feeding::{pictured, watched, worker};

const SIZE: Size = Size { cols: 20, rows: 5 };

fn lines() -> String {
    super::feeding::lines(0..60) + "$ "
}

/// Answers asks for rows, until there are none, and gives back any other ask.
fn fetched(window: &mut Window, worker: &mut Mirror, asks: Vec<Ask>) -> Vec<Ask> {
    let (mut asks, mut others) = (asks, Vec::new());
    while let Some(ask) = asks.pop() {
        match ask {
            Ask::Rows { numbering, from, count } => {
                asks.extend(window.replied(Replied::Rows(worker.history_rows(numbering, from, count).unwrap())));
            }
            ask @ (Ask::Typed(_) | Ask::Fit { .. } | Ask::Bounds { .. }) => others.push(ask),
        }
    }
    others
}

fn text(window: &mut Window) -> Vec<String> {
    window.frame().unwrap().rows.iter().map(|row| row.text()).collect()
}

/// A window scrolled back to "line 30", with "line 31" selected, and what it took to get there.
fn scrolled_and_selected() -> (Mirror, PictureSending, Window) {
    let mut worker = worker(SIZE, lines().as_bytes());
    let mut sending = PictureSending::default();
    let mut window = watched(&mut worker, &mut sending);
    let asks = window.wheel(-26);
    assert!(fetched(&mut window, &mut worker, asks).is_empty());
    assert_eq!(text(&mut window)[0], "line 30");
    window.press(Pointer { col: 0.1, row: 1.5 }, SelectBy::Cells);
    window.drag(Pointer { col: 6.9, row: 1.5 });
    window.release();
    (worker, sending, window)
}

/// What the worker does with a fit, as `TerminalFit` says.
fn fit(worker: &mut Mirror, ask: &Ask) -> Replied {
    let Ask::Fit { size, numbering, keep } = ask else { panic!("asked for {ask:?}") };
    let tracked = worker.track(*numbering, keep).unwrap();
    worker.resize(Size { cols: size.cols, rows: size.rows }).unwrap();
    Replied::Fit { size: *size, asked: keep.clone(), reply: worker.tracked(&tracked).unwrap() }
}

fn take(window: &mut Window, worker: &mut Mirror, frames: Vec<TerminalPicture>) {
    for frame in frames {
        let asks = window.take(frame).unwrap();
        assert!(fetched(window, worker, asks).is_empty());
    }
}

fn selected_text(window: &Window, worker: &mut Mirror) -> String {
    let (numbering, range) = window.selected().unwrap();
    worker.text(numbering, range, false).unwrap().text
}

#[test]
fn a_new_size_this_window_asked_for_keeps_its_place_and_selection_whichever_comes_first() {
    for reply_first in [true, false] {
        let (mut worker, mut sending, mut window) = scrolled_and_selected();
        let asks = window.fit(Size { cols: 30, rows: 6 });
        let [ask] = asks.as_slice() else { panic!("asked for {asks:?}") };
        let replied = fit(&mut worker, ask);
        let picture = pictured(&mut worker, &mut sending);

        if reply_first {
            assert!(window.replied(replied).is_empty());
            take(&mut window, &mut worker, picture);
        } else {
            take(&mut window, &mut worker, picture);
            let asks = window.replied(replied);
            assert!(fetched(&mut window, &mut worker, asks).is_empty());
        }

        assert_eq!(text(&mut window)[0], "line 30", "with the reply first: {reply_first}");
        assert_eq!(selected_text(&window, &mut worker), "line 31");
    }
}

#[test]
fn a_new_size_another_device_gave_takes_the_window_to_the_bottom_with_nothing_selected() {
    let (mut worker, mut sending, mut window) = scrolled_and_selected();

    worker.resize(Size { cols: 30, rows: 6 }).unwrap();
    let picture = pictured(&mut worker, &mut sending);
    take(&mut window, &mut worker, picture);

    assert_eq!(text(&mut window).last().map(String::as_str), Some("$"));
    assert_eq!(window.selected(), None);
}

#[test]
fn the_same_size_again_asks_for_nothing() {
    let (_worker, _sending, mut window) = scrolled_and_selected();

    assert!(window.fit(SIZE).is_empty());
    assert!(window.fit(SIZE).is_empty());
}

/// So a view that watches again, as on a new connection, takes the terminal back to its own size, but keeps one
/// another device gave it while the view's room stays as it was.
#[test]
fn a_fresh_picture_at_another_size_than_the_views_room_asks_for_the_room() {
    let mut worker = worker(SIZE, lines().as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    assert!(window.fit(SIZE).is_empty());

    worker.resize(Size { cols: 30, rows: 6 }).unwrap();
    let again = pictured(&mut worker, &mut PictureSending::default());
    let asks: Vec<Ask> = again.into_iter().flat_map(|frame| window.take(frame).unwrap()).collect();

    let fits: Vec<_> =
        asks.iter().filter_map(|ask| if let Ask::Fit { size, .. } = ask { Some(*size) } else { None }).collect();
    assert_eq!(fits.iter().map(|size| (size.cols, size.rows)).collect::<Vec<_>>(), [(SIZE.cols, SIZE.rows)]);
}

#[test]
fn typing_takes_the_window_to_the_bottom_and_clears_what_was_selected() {
    let (_worker, _sending, mut window) = scrolled_and_selected();

    let asks = window.typed(Typed::Text { text: "ls\r".into() });

    assert_eq!(asks, [Ask::Typed(Typed::Text { text: "ls\r".into() })]);
    assert_eq!(text(&mut window).last().map(String::as_str), Some("$"));
    assert_eq!(window.selected(), None);
}

/// A pager's screen has no history, so the wheel is sent, for the worker to turn into the arrow keys.
#[test]
fn the_wheel_on_a_programs_screen_is_sent_to_it() {
    let mut worker = worker(SIZE, b"$ less\r\n\x1b[?1049h\x1b[?1007h\x1b[Hpage");
    let mut window = watched(&mut worker, &mut PictureSending::default());

    assert_eq!(window.wheel(-3), [Ask::Typed(Typed::Wheel { lines: -3 })]);
}

/// Rather than leaving the terminal at the size between, and a reply to the fit before says nothing of where the view
/// goes.
#[test]
fn room_that_goes_back_before_the_new_size_has_come_asks_for_its_size_again() {
    let (mut worker, _sending, mut window) = scrolled_and_selected();
    assert!(window.fit(SIZE).is_empty());
    let wider = window.fit(Size { cols: 30, rows: 5 });
    let replied = fit(&mut worker, &wider[0]);

    let back = window.fit(SIZE);

    let [Ask::Fit { size, .. }] = back.as_slice() else { panic!("asked for {back:?}") };
    assert_eq!((size.cols, size.rows), (SIZE.cols, SIZE.rows));
    assert!(window.replied(replied).is_empty());
    assert_eq!(text(&mut window)[0], "line 30", "the view stays until the fit it's waiting for comes");
}

/// As after the hub restarts: the window watches again, and is sent a picture from nothing in the same numbering, so
/// the rows it held could have changed, but a place by number holds.
#[test]
fn a_picture_from_nothing_in_the_same_numbering_keeps_the_place_and_what_was_selected() {
    let (mut worker, _sending, mut window) = scrolled_and_selected();

    window.disconnected();
    let picture = pictured(&mut worker, &mut PictureSending::default());
    take(&mut window, &mut worker, picture);

    assert_eq!(text(&mut window)[0], "line 30");
    assert_eq!(selected_text(&window, &mut worker), "line 31");
}

/// It can't say where the view goes until then, so it shows what it had.
#[test]
fn a_picture_at_the_new_size_before_the_fits_reply_waits_for_it() {
    let (mut worker, mut sending, mut window) = scrolled_and_selected();
    let _ = window.frame();
    let asks = window.fit(Size { cols: 30, rows: 6 });
    let replied = fit(&mut worker, &asks[0]);

    let picture = pictured(&mut worker, &mut sending);
    take(&mut window, &mut worker, picture);

    assert!(window.frame().is_none(), "the view goes on showing what it had");
    assert_eq!(window.selected(), None, "since its ends are in the numbering before");
    assert!(window.wheel(-1).is_empty());
    window.replied(replied);
    assert_eq!(text(&mut window)[0], "line 30");
}

/// What's typed meanwhile takes the view to the bottom and clears the selection, which the fit's reply doesn't undo.
#[test]
fn typing_while_a_fit_is_on_its_way_isnt_undone_by_its_reply() {
    let (mut worker, mut sending, mut window) = scrolled_and_selected();
    let asks = window.fit(Size { cols: 30, rows: 6 });
    let replied = fit(&mut worker, &asks[0]);

    window.typed(Typed::Text { text: "x".into() });
    window.replied(replied);
    let picture = pictured(&mut worker, &mut sending);
    take(&mut window, &mut worker, picture);

    assert_eq!(window.selected(), None);
    assert_eq!(text(&mut window).last().map(String::as_str), Some("$"));
}
