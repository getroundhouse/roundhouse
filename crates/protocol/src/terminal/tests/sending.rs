//! What a sender of a terminal's pictures sends its receiver, from what it has.

use crate::terminal::{
    CursorShape, Numbering, PictureSending, RowJson, RowNumber, RowRun, ScreenState, Shown, Span, TerminalCursor,
    TerminalPicture, TerminalRow, TerminalSize,
};

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
        cursor: Some(TerminalCursor { col: 0, row: 0, shape: CursorShape::Block, blinking: false }),
        state: ScreenState::default(),
    }
}

/// The rows a frame carries, by number, as text.
fn rows_of(picture: &TerminalPicture) -> Vec<(u64, String)> {
    let each = |run: &RowRun| {
        let numbers = run.from.get()..;
        numbers.zip(&run.rows).map(|(number, row)| (number, row.row().spans[0].text.clone())).collect::<Vec<_>>()
    };
    picture.rows.iter().flat_map(each).collect()
}

fn sent(text: &[(u64, &str)]) -> Vec<(u64, String)> {
    text.iter().map(|(number, text)| (*number, (*text).to_owned())).collect()
}

/// No history at all, as for a sender that never needs it.
fn none(_: RowNumber) -> Option<RowJson> {
    None
}

#[test]
fn a_receiver_with_nothing_is_sent_the_whole_screen_and_then_only_what_changed() {
    let mut sending = PictureSending::default();

    let first = sending.next(shown(200, &["a", "b", "c"]), none).unwrap();
    let changed = sending.next(shown(200, &["a", "B", "c"]), none).unwrap();
    let again = sending.next(shown(200, &["a", "B", "c"]), none);

    assert!(first.whole && first.state.is_some() && first.cursor.is_some(), "{first:?}");
    assert_eq!(rows_of(&first), sent(&[(200, "a"), (201, "b"), (202, "c")]));
    assert_eq!(first.kept_from, RowNumber::new(100), "nothing it holds is known to be wrong");
    assert!(changed.whole && changed.state.is_none(), "{changed:?}");
    assert_eq!(rows_of(&changed), sent(&[(201, "B")]));
    assert_eq!(again, None);
}

/// A scrolled row is the same row under the same number, so a scroll by a line sends one row.
#[test]
fn a_scroll_sends_only_the_rows_that_came_onto_the_screen() {
    let mut sending = PictureSending::default();
    sending.next(shown(200, &["a", "b", "c"]), none).unwrap();

    let history = |number: RowNumber| (number.get() == 200).then(|| row("a"));
    let scrolled = sending.next(shown(201, &["b", "c", "d"]), history).unwrap();

    assert_eq!(rows_of(&scrolled), sent(&[(203, "d")]));
    assert_eq!((scrolled.top, scrolled.kept_from), (RowNumber::new(201), RowNumber::new(100)));
}

/// A row that changed after the receiver's copy and then left the screen is history the receiver has wrong, so it
/// lets go of it and everything above, and one that came and went between pictures it never had.
#[test]
fn history_the_receiver_has_wrong_or_lacks_is_said() {
    let mut changed = PictureSending::default();
    changed.next(shown(200, &["a", "b", "c"]), none).unwrap();
    let mut flooded = PictureSending::default();
    flooded.next(shown(200, &["a", "b", "c"]), none).unwrap();

    let history = |number: RowNumber| match number.get() {
        200 => Some(row("a")),
        201 => Some(row("b, changed")),
        _ => None,
    };
    let after_change = changed.next(shown(202, &["c", "d", "e"]), history).unwrap();
    let after_flood = flooded.next(shown(400, &["x", "y", "z"]), none).unwrap();

    assert_eq!(after_change.kept_from, RowNumber::new(202));
    assert_eq!(after_flood.kept_from, RowNumber::new(400));
}

#[test]
fn a_new_numbering_or_size_is_sent_whole() {
    let mut sending = PictureSending::default();
    sending.next(shown(200, &["a", "b"]), none).unwrap();

    let renumbered = sending.next(Shown { numbering: Numbering::new(2), ..shown(200, &["a", "b"]) }, none).unwrap();
    let resized = sending.next(shown(200, &["a", "b", "c"]), none).unwrap();

    assert_eq!(rows_of(&renumbered), sent(&[(200, "a"), (201, "b")]));
    assert_eq!(rows_of(&resized).len(), 3);
    assert!(renumbered.state.is_some(), "the receiver's state was of another picture");
}

/// Back from a program's screen, the shell's rows the receiver had are of a numbering it was since sent another of, so
/// the sender vouches for nothing it can't know, and the receiver has let go of what was on its screen, as
/// `TerminalPicture::kept_from` says.
#[test]
fn back_from_another_numbering_only_the_rows_kept_are_vouched_for() {
    let mut sending = PictureSending::default();
    sending.next(shown(200, &["$ les"]), none).unwrap();
    sending
        .next(Shown { numbering: Numbering::new(2), first: RowNumber::new(0), ..shown(0, &["a page"]) }, none)
        .unwrap();

    let back = sending.next(shown(300, &["$ "]), none).unwrap();

    assert_eq!(back.kept_from, back.first);
    assert_eq!(rows_of(&back), sent(&[(300, "$ ")]));
}

#[test]
fn a_cursor_or_state_that_changed_alone_is_sent_with_no_rows() {
    let mut sending = PictureSending::default();
    sending.next(shown(200, &["a"]), none).unwrap();

    let moved =
        Shown { cursor: Some(TerminalCursor { col: 4, ..shown(0, &[]).cursor.unwrap() }), ..shown(200, &["a"]) };
    let moved = sending.next(moved, none).unwrap();
    let titled = ScreenState { title: "vim".into(), ..ScreenState::default() };
    let titled = sending.next(Shown { state: titled, ..shown(200, &["a"]) }, none).unwrap();

    assert!(moved.rows.is_empty() && moved.whole && moved.state.is_none(), "{moved:?}");
    assert_eq!(moved.cursor.map(|cursor| cursor.col), Some(4));
    assert!(titled.rows.is_empty() && titled.state.as_ref().is_some_and(|state| state.title == "vim"), "{titled:?}");
}

/// Otherwise a large screen under a flood would never be whole, since every row would be new by the next frame.
#[test]
fn a_picture_too_big_for_a_frame_is_finished_as_it_began() {
    let long = "x".repeat(10 * 1024);
    let texts: Vec<String> = (0..20).map(|n| format!("{n} {long}")).collect();
    let screen: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut sending = PictureSending::default();

    let mut frames = vec![sending.next(shown(200, &screen), none).unwrap()];
    // Twenty rows of 10 KiB go in a few frames, and a sender that starts again never ends.
    while sending.sending() && frames.len() < 20 {
        let newer: Vec<String> = texts.iter().map(|text| format!("newer {text}")).collect();
        let newer: Vec<&str> = newer.iter().map(String::as_str).collect();
        frames.push(sending.next(shown(200, &newer), none).unwrap());
    }

    let rows: Vec<_> = frames.iter().flat_map(rows_of).collect();
    assert_eq!(rows, texts.iter().zip(200..).map(|(text, number)| (number, text.clone())).collect::<Vec<_>>());
    assert!(frames.len() > 1, "{} frames", frames.len());
    assert!(
        frames.iter().all(|frame| frame
            .rows
            .iter()
            .map(|run| run.rows.iter().map(RowJson::len).sum::<usize>())
            .sum::<usize>()
            <= crate::terminal::MOST_PICTURE_BYTES)
    );
    let (last, before) = frames.split_last().unwrap();
    assert!(last.whole && last.cursor.is_some() && last.state.is_some(), "{last:?}");
    assert!(before.iter().all(|frame| !frame.whole && frame.cursor.is_none() && frame.state.is_none()));
}
