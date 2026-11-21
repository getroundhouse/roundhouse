//! The hub's copy of a watched terminal, from pictures a worker's sender makes.

use protocol::terminal::{
    Numbering, PictureSending, RowJson, RowNumber, ScreenState, Shown, Span, TerminalPicture, TerminalRow, TerminalSize,
};

use crate::hub::terminals::copy::{Copy, Incomplete};

fn row(text: &str) -> RowJson {
    RowJson::new(&TerminalRow { spans: vec![Span { text: text.into(), ..Span::default() }], ..TerminalRow::default() })
        .unwrap()
}

/// A screen of `rows`, its first numbered `top`, with 100 rows of history above.
fn shown(top: u64, rows: &[&str]) -> Shown {
    Shown {
        numbering: Numbering::new(1),
        size: TerminalSize { cols: 80, rows: u16::try_from(rows.len()).unwrap() },
        top: RowNumber::new(top),
        first: RowNumber::new(top.saturating_sub(100)),
        rows: rows.iter().map(|text| row(text)).collect(),
        cursor: None,
        state: ScreenState::default(),
    }
}

fn none(_: RowNumber) -> Option<RowJson> {
    None
}

/// Every frame of the next picture the worker's sender makes of `now`, for a hub that has what it was sent.
fn frames(
    sending: &mut PictureSending,
    now: Shown,
    history: impl FnMut(RowNumber) -> Option<RowJson>,
) -> Vec<TerminalPicture> {
    let mut frames: Vec<_> = sending.next(now, history).into_iter().collect();
    frames.extend(std::iter::from_fn(|| sending.rest()));
    frames
}

fn take_all(copy: &mut Copy, frames: Vec<TerminalPicture>) {
    for frame in frames {
        copy.take(frame).unwrap();
    }
}

fn screen(copy: &Copy) -> Option<Vec<String>> {
    let shown = copy.watched().borrow().shown.clone()?;
    Some(shown.rows.iter().map(|row| row.row().spans[0].text.clone()).collect())
}

#[test]
fn the_copy_is_the_screen_of_the_newest_whole_picture() {
    let (mut copy, mut sending) = (Copy::default(), PictureSending::default());

    take_all(&mut copy, frames(&mut sending, shown(200, &["a", "b", "c"]), none));
    let first = screen(&copy);
    take_all(&mut copy, frames(&mut sending, shown(200, &["a", "B", "c"]), none));

    assert_eq!(first, Some(vec!["a".into(), "b".into(), "c".into()]));
    assert_eq!(screen(&copy), Some(vec!["a".into(), "B".into(), "c".into()]));
}

/// Only a whole picture is shown, since one partway through has some rows of one moment and some of another.
#[test]
fn a_picture_shows_only_once_its_last_frame_has_come() {
    let (mut copy, mut sending) = (Copy::default(), PictureSending::default());
    let long = "x".repeat(20 * 1024);
    let texts: Vec<String> = (0..10).map(|n| format!("{n} {long}")).collect();
    let rows: Vec<&str> = texts.iter().map(String::as_str).collect();

    let mut frames = frames(&mut sending, shown(200, &rows), none);
    let last = frames.pop().unwrap();
    take_all(&mut copy, frames);
    let before_the_last = screen(&copy);
    copy.take(last).unwrap();

    assert_eq!(before_the_last, None);
    assert_eq!(screen(&copy), Some(texts));
}

/// Rows that left the screen are kept from where the worker says they're history as it is, so a client's copy of one
/// can be checked, and one the worker says changed before it left isn't.
#[test]
fn rows_that_left_the_screen_are_kept_only_while_the_worker_vouches_for_them() {
    let (mut copy, mut sending) = (Copy::default(), PictureSending::default());
    take_all(&mut copy, frames(&mut sending, shown(200, &["a", "b", "c"]), none));

    let history = |number: RowNumber| match number.get() {
        200 => Some(row("a")),
        201 => Some(row("b, changed")),
        _ => None,
    };
    take_all(&mut copy, frames(&mut sending, shown(202, &["c", "d", "e"]), history));

    let kept = copy.kept();
    let kept = kept.lock();
    assert_eq!(kept.row(Numbering::new(1), RowNumber::new(200)), None, "kept from 202, past the changed row");
    assert_eq!(kept.row(Numbering::new(1), RowNumber::new(201)), None);
    drop(kept);
    take_all(&mut copy, frames(&mut sending, shown(203, &["d", "e", "f"]), |_| Some(row("c"))));
    assert_eq!(copy.kept().lock().row(Numbering::new(1), RowNumber::new(202)), Some(row("c")));
    assert_eq!(copy.kept().lock().row(Numbering::new(2), RowNumber::new(202)), None, "another numbering's");
}

/// Once the worker is asked to watch again, what it sent before it began again is for a watch that ended, which the
/// frames of a picture can't say, so the copy takes nothing until the fresh picture it begins with.
#[test]
fn after_the_worker_is_asked_again_what_came_before_the_fresh_picture_is_let_go() {
    let (mut copy, mut stale) = (Copy::default(), PictureSending::default());
    take_all(&mut copy, frames(&mut stale, shown(200, &["a", "b"]), none));

    copy.asked();
    take_all(&mut copy, frames(&mut stale, shown(200, &["stale", "b"]), none));
    take_all(&mut copy, frames(&mut stale, shown(300, &["stale", "screen"]), none));
    let before_the_fresh = screen(&copy);
    let mut fresh = PictureSending::default();
    take_all(&mut copy, frames(&mut fresh, shown(300, &["new", "screen"]), none));

    assert_eq!(before_the_fresh, Some(vec!["a".into(), "b".into()]));
    assert_eq!(screen(&copy), Some(vec!["new".into(), "screen".into()]));
}

/// A picture that leaves out a row the copy doesn't have, as one made for another receiver would, can't be shown, so
/// the copy waits for a fresh one.
#[test]
fn a_picture_lacking_a_row_the_copy_doesnt_have_waits_for_a_fresh_one() {
    let (mut copy, mut sending) = (Copy::default(), PictureSending::default());
    take_all(&mut copy, frames(&mut sending, shown(200, &["a", "b"]), none));
    let mut other = PictureSending::default();
    frames(&mut other, shown(200, &["a", "b", "c"]), none);

    let lacking = frames(&mut other, shown(200, &["a", "b", "C"]), none);
    let taken: Vec<_> = lacking.into_iter().map(|frame| copy.take(frame)).collect();
    let after = frames(&mut sending, shown(200, &["a", "B"]), none);
    take_all(&mut copy, after);

    assert_eq!(taken, [Err(Incomplete)]);
    assert!(copy.wants_fresh());
    assert_eq!(screen(&copy), Some(vec!["a".into(), "b".into()]), "nothing until a fresh one");
}
