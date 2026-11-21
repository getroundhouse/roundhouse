use gpui_kit::{Bounds, Pixels, TestAppContext, TextLayout, point, px};
use protocol::model::Seq;

use super::super::super::test_layouts::{in_pieces, one_above_another, side_by_side};
use super::super::super::{Drawn, End, Geometry, Pieces};
use super::super::Way;
use super::{Line, Run, move_a_line};
use crate::texts::{Leaves, TextAt, TextKey};

/// A text drawn whole, as the selection sees it once it's drawn: `pieces`, at `at` in the chat, with what shows of it
/// `shows`, or all of it.
fn drawn(text: &str, at: u64, pieces: Pieces, shows: Option<Bounds<Pixels>>) -> Drawn {
    let bounds = shows.unwrap_or_else(|| Geometry::Plain(pieces.clone()).bounds());
    Drawn {
        text: TextKey::named(text.to_string()),
        at: TextAt::only(Seq::new(at), 0),
        bounds,
        area: None,
        leaves: vec![(0, Geometry::Plain(pieces))],
    }
}

/// What the texts in a test show: each one the same, `said`.
fn showing(said: &'static str) -> impl Fn(&TextKey) -> Option<Leaves> {
    move |_| Some(Leaves::Plain(said.into()))
}

fn end(text: &str, byte: usize) -> End {
    End { text: TextKey::named(text.to_string()), leaf: 0, byte }
}

/// Where a move from `from` goes, `way`, in the run `run`, among `texts`, which each show `shows`, as a run.
fn moved(texts: &[Drawn], shows: &'static str, from: &End, run: Option<&Run>, way: Way) -> Run {
    match move_a_line(texts, &showing(shows), from, run, way, false) {
        Line::To(run) => run,
        other @ (Line::Past(_) | Line::NotDrawn) => panic!("{from:?} went nowhere {way:?}: {other:?}"),
    }
}

/// Each place in `layout`, which lays out `text`, that a row wraps at, which GPUI draws at the end of the row before.
fn wraps(layout: &TextLayout, text: &str) -> Vec<usize> {
    let y = |index| layout.position_for_index(index).map(|at| at.y);
    (0..text.len()).filter(|index| y(*index) < y(index + 1)).collect()
}

/// A move up or down goes to the row above or below, through the space between two lines of a text, to the place
/// nearest across from where the end was, which a run of them keeps to, and a place on a shorter row is its end.
#[gpui_kit::test]
fn a_line_above_or_below_is_found_across_from_where_the_end_was(cx: &mut TestAppContext) {
    let (pieces, layouts) = in_pieces(cx.add_empty_window(), &["abcdef", "ab", "abcdef"], 6.);
    let (texts, said) = ([drawn("output", 1, pieces, None)], "abcdef\nab\nabcdef");
    let at_d = layouts[0].position_for_index(3).unwrap().x;

    let down = moved(&texts, said, &end("output", 3), None, Way::On);
    assert_eq!(down.end, end("output", 9), "the end of the shorter line below, past the line break after the first");
    assert_eq!(down.across, at_d, "and it keeps to where it was across");
    let again = moved(&texts, said, &down.end, Some(&down), Way::On);
    assert_eq!(again.end, end("output", 13), "so the next goes back across from where it started");
    let up = moved(&texts, said, &again.end, Some(&again), Way::Back);
    assert_eq!(up.end, end("output", 9), "and up comes back the same way");

    let at_b = layouts[0].position_for_index(1).unwrap().x;
    let first = Run { end: end("output", 1), across: at_b, wrapped: false };
    assert_eq!(
        move_a_line(&texts, &showing(said), &end("output", 1), None, Way::Back, false),
        Line::Past(Some(first)),
        "nothing's above the first line among what's drawn, and where it was across is kept for once there is"
    );
    let elsewhere = move_a_line(&texts, &showing(said), &end("elsewhere", 0), None, Way::On, false);
    assert_eq!(elsewhere, Line::NotDrawn, "and an end that isn't drawn has to be first");
}

/// GPUI draws the place where a row wraps at the end of the row before, so a move down from a line's start, which is
/// across from that place, is at the start of the next row, and the next move goes on from there, rather than finding
/// the same row again. Up from the start of the line below a wrapped one is at the start of its last row.
#[gpui_kit::test]
fn a_move_down_from_the_start_of_a_line_that_wraps_goes_to_the_start_of_each_row_in_turn(cx: &mut TestAppContext) {
    let long = "trains wait at a stop signal until it clears and the line ahead is free of other trains";
    let (pieces, layouts) = in_pieces(cx.add_empty_window(), &[long, "next"], 6.);
    let said: &'static str = Box::leak(format!("{long}\nnext").into_boxed_str());
    let texts = [drawn("output", 1, pieces, None)];
    let rows = wraps(&layouts[0], long);
    assert!(rows.len() >= 2, "it wraps onto three rows or more in the window: {rows:?}");

    let second = moved(&texts, said, &end("output", 0), None, Way::On);
    let third = moved(&texts, said, &second.end, Some(&second), Way::On);
    let up = moved(&texts, said, &end("output", long.len() + 1), None, Way::Back);

    assert_eq!((second.end.byte, second.wrapped), (rows[0], true), "the start of the second row");
    assert_eq!((third.end.byte, third.wrapped), (rows[1], true), "and of the third");
    assert_eq!((up.end.byte, up.wrapped), (*rows.last().unwrap(), true), "and up, the start of the last");
}

/// What an area that scrolls a text has moved out of sight is laid out, and is the line to move to, which the area
/// then scrolls into view, rather than the next text that shows.
#[gpui_kit::test]
fn a_line_that_an_area_has_moved_out_of_sight_is_one_to_move_to(cx: &mut TestAppContext) {
    let (pieces, layouts) = in_pieces(cx.add_empty_window(), &["one", "two", "three"], 6.);
    let (first, second) = (layouts[0].bounds(), layouts[1].bounds());
    // What shows of it stops just above its second line, and another text shows below.
    let shows = Bounds::from_corners(first.origin, point(px(200.), second.top() - px(1.)));
    let texts = [drawn("cut", 1, pieces.clone(), Some(shows)), drawn("below", 2, pieces, None)];

    let down = moved(&texts, "one\ntwo\nthree", &end("cut", 1), None, Way::On);

    assert_eq!(down.end, end("cut", 5), "its second line, which doesn't show");
}

/// A table's cells sit side by side in its rows, so a line below one in a cell is in the cell under it, even where
/// the cell beside it goes on onto a line that's nearer.
#[gpui_kit::test]
fn a_move_down_in_a_table_goes_to_the_cell_under_rather_than_the_one_beside(cx: &mut TestAppContext) {
    let long = "what each signal says to a driver who sees it";
    let cells = side_by_side(cx.add_empty_window(), &[["Name", long], ["Next", "clear"]]);
    let texts: Vec<Drawn> = ((1..).zip(cells.iter().flatten()))
        .map(|(at, (text, layout))| drawn(text, at, Pieces::whole(layout.clone()), None))
        .collect();
    let shows = |key: &TextKey| {
        let text = cells.iter().flatten().find(|(text, _)| TextKey::named(text.to_string()) == *key)?.0;
        Some(Leaves::Plain(text.into()))
    };
    assert!(cells[0][1].1.bounds().size.height > cells[0][0].1.bounds().size.height, "the cell beside it wraps");

    let down = match move_a_line(&texts, &shows, &end("Name", 4), None, Way::On, false) {
        Line::To(run) => run.end,
        other @ (Line::Past(_) | Line::NotDrawn) => panic!("{other:?}"),
    };

    assert_eq!(down.text, TextKey::named("Next"), "the cell under it");
}

/// Once the list has drawn the rows past the end for a move, and the end's text is no longer drawn, the line that way
/// is the first of the texts drawn, at where across the run keeps to.
#[gpui_kit::test]
fn past_the_end_the_line_is_the_first_of_the_texts_drawn(cx: &mut TestAppContext) {
    let (pieces, layouts) = in_pieces(cx.add_empty_window(), &["abcdef", "ab"], 6.);
    let texts = [drawn("below", 2, pieces, None)];
    let across = layouts[0].position_for_index(2).unwrap().x;
    let run = Run { end: end("gone", 0), across, wrapped: false };

    let moved = move_a_line(&texts, &showing("abcdef\nab"), &end("gone", 0), Some(&run), Way::On, true);

    assert_eq!(moved, Line::To(Run { end: end("below", 2), across, wrapped: false }));
}

/// What an area hides of another text is laid out where the texts below the area are drawn, so it isn't a line to go
/// to, as it isn't one to drag to: from a text below an area, a move goes through that text's own lines, and up from
/// its first, to the last line the area shows.
#[gpui_kit::test]
fn what_an_area_hides_of_another_text_is_not_a_line_to_go_to(cx: &mut TestAppContext) {
    const OUTPUT: [&str; 10] = ["101", "102", "103", "104", "105", "106", "107", "108", "109", "110"];
    const REPLY: [&str; 2] = ["first paragraph", "second paragraph"];
    let window = cx.add_empty_window();
    let (lines, _) = one_above_another(window, &OUTPUT, &REPLY, 1000.);
    // The area shows the first three lines, and the reply is drawn just below it, over where the others are laid out.
    let cut = lines[2].bounds().bottom();
    let (lines, paragraphs) = one_above_another(window, &OUTPUT, &REPLY, cut.as_f32() + 28.);
    let shows = Bounds::from_corners(lines[0].bounds().origin, point(px(200.), cut));
    let texts = [
        drawn("output", 1, pieces(&OUTPUT, &lines), Some(shows)),
        drawn("reply", 2, pieces(&REPLY, &paragraphs), None),
    ];
    let said = |key: &TextKey| {
        let lines: &[&str] = if *key == TextKey::named("output") { &OUTPUT } else { &REPLY };
        Some(Leaves::Plain(lines.join("\n").into()))
    };
    let move_from = |end: End, way| match move_a_line(&texts, &said, &end, None, way, false) {
        Line::To(run) => run.end,
        other @ (Line::Past(_) | Line::NotDrawn) => panic!("{other:?}"),
    };
    let (top, bottom) = (paragraphs[0].bounds().top(), paragraphs[1].bounds().bottom());
    let beside = lines[3..].iter().filter(|line| line.bounds().top() < bottom && line.bounds().bottom() > top).count();
    assert!(beside >= 2, "the lines the area hides are laid out beside the reply's");

    let down = move_from(end("reply", REPLY[0].len()), Way::On);
    let up = move_from(end("reply", 0), Way::Back);

    assert_eq!(down.text, TextKey::named("reply"), "down goes to the reply's second paragraph: {down:?}");
    assert_eq!(up.text, TextKey::named("output"), "and up from its first, to the output");
    assert!((8..=11).contains(&up.byte), "on the last line the area shows, 103: {up:?}");
}

/// The pieces of one text, `texts`, laid out in `layouts`.
fn pieces(texts: &[&'static str], layouts: &[TextLayout]) -> Pieces {
    let mut each = layouts.iter().cloned();
    Pieces::lines(texts.iter().copied(), |_, _| each.next().expect("a layout for each piece"))
}
