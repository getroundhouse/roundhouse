use gpui_kit::{
    Bounds, IntoElement as _, Pixels, Point, StyledText, TestAppContext, VisualTestContext, point, px, size,
};

use super::super::test_layouts::{in_pieces, laid_out};
use super::{Pieces, Span, boxes, highlight};

/// A press in plain text goes to the side of a character it's nearer, as one in Markdown does, so a drag selects the
/// same characters in either. Past a line's end, or below the last, it goes to the nearest place on the nearest line.
#[gpui_kit::test]
fn a_press_goes_to_the_nearer_side_of_the_character_under_it(cx: &mut TestAppContext) {
    let layout = laid_out(cx.add_empty_window(), &["abc\nde"], 0.).remove(0);
    let pieces = Pieces::whole(layout.clone());
    let start = |index| layout.position_for_index(index).expect("it's laid out");
    let (width, row) = (start(1).x - start(0).x, layout.line_height());
    assert!(width > Pixels::ZERO, "its characters have room");
    // A point a fraction of the way across the character at `index`, half way down its row.
    let across = |index: usize, fraction: f32| start(index) + point(width * fraction, row / 2.);
    let at = |point: Point<Pixels>| pieces.index_for_position(point, pieces.bounds());

    assert_eq!(at(across(0, 0.25)), 0, "the first character's left half is before it");
    assert_eq!(at(across(0, 0.75)), 1, "and its right half after it");
    assert_eq!(at(across(1, 0.25)), 1);
    assert_eq!(at(across(4, 0.75)), 5, "on the second line too");
    assert_eq!(at(start(0) + point(px(150.), row / 2.)), 3, "past the first line's end is its end");
    assert_eq!(at(start(4) + point(px(1.), row * 3.)), 4, "below the last line is on it");
    assert_eq!(at(start(0) - point(px(5.), px(5.))), 0, "and above and left of it is its start");
}

/// A text laid out a line a piece, with space between: a place in the second piece counts from the start of the text,
/// past the line break between. A point in the space between is on the next piece, if it shows, and otherwise on the
/// one above, so where an area cuts the text short there, no point is taken to be in what's cut off.
#[gpui_kit::test]
fn a_place_in_a_piece_counts_from_the_start_of_the_text(cx: &mut TestAppContext) {
    let (pieces, layouts) = in_pieces(cx.add_empty_window(), &["abc", "de"], 6.);
    let (all, top, above) = (pieces.bounds(), layouts[1].bounds().top(), layouts[0].bounds().bottom());
    let (row, between) = (layouts[1].line_height(), point(px(150.), (above + top) / 2.));
    let second = point(px(150.), top + row / 2.);
    // What shows of them where an area cuts them short at `bottom`, and where one cuts them at their top between them.
    let down_to = |bottom: Pixels| Bounds::from_corners(all.origin, point(all.right(), bottom));
    let below = Bounds::from_corners(point(all.left(), between.y), all.bottom_right());
    let at = |point: Point<Pixels>, shows: Bounds<Pixels>| pieces.index_for_position(point, shows);

    assert_eq!(pieces.len(), 6);
    assert_eq!(at(point(px(150.), row / 2.), all), 3, "the end of the first");
    assert_eq!(at(second, all), 6, "the end of the second");
    assert_eq!(at(between, all), 6, "between them, past their ends, a point is on the second");
    assert_eq!(at(between, down_to(between.y + px(1.))), 3, "and on the first, if it's cut off between them");
    assert_eq!(at(second, down_to(second.y + px(1.))), 6, "but not if it's cut in the second");
    assert_eq!(at(point(px(0.), between.y), below), 4, "and cut off above, between them, it's the second's start");
    assert_eq!(all.bottom(), layouts[1].bounds().bottom(), "and they're as tall as both");
}

/// The debug check that a text's pieces lay out what it says, so a place in it is where it's painted.
#[gpui_kit::test]
fn pieces_lay_out_only_what_their_text_says(cx: &mut TestAppContext) {
    let (pieces, _) = in_pieces(cx.add_empty_window(), &["abc", "de"], 0.);

    assert!(pieces.lay_out("abc\nde"));
    assert!(!pieces.lay_out("abc de"), "a line break between them, not anything else");
    assert!(!pieces.lay_out("abc\nd"), "all of each piece");
    assert!(!pieces.lay_out("abc\nde\n"), "and nothing after the last");
    assert!(!pieces.lay_out("abc\nxy"), "each piece laid out as what it says");
}

/// Each piece is laid out knowing where it is in the text, so what's found in the text is drawn in the piece it's in.
/// What a command printed, `one`, a blank line and `three`, in pieces as `rows::tools::block` lays it out.
#[gpui_kit::test]
fn each_piece_is_laid_out_knowing_where_it_is_in_the_text(cx: &mut TestAppContext) {
    let texts = ["one\n", "three"];
    let layouts = laid_out(cx.add_empty_window(), &texts, 0.);
    let (mut given, mut each) = (Vec::new(), layouts.into_iter());

    Pieces::lines(texts, |within, text| {
        given.push((within, text));
        each.next().expect("a layout for each")
    });

    assert_eq!(given, [(0..4, "one\n"), (5..10, "three")]);
}

#[test]
fn a_selection_within_a_row_is_one_box_and_across_rows_reaches_the_line_s_edges() {
    let line = Bounds::from_corners(point(px(0.), px(0.)), point(px(100.), px(60.)));

    assert_eq!(highlight(point(px(10.), px(0.)), point(px(40.), px(0.)), line, px(20.)).len(), 1);

    let across = highlight(point(px(10.), px(0.)), point(px(40.), px(40.)), line, px(20.));
    assert_eq!(across.len(), 3);
    assert_eq!(across[0].right(), px(100.), "the first row to the line's right edge");
    assert_eq!(across[2].origin, Point::new(px(0.), px(40.)), "the last from its left edge");

    let next = highlight(point(px(10.), px(0.)), point(px(40.), px(20.)), line, px(20.));
    assert_eq!(next.len(), 2, "with no rows between, there's nothing between to fill");
}

/// Three pieces of one row each, 4 apart: selected from part way into the first to part way into the last, through
/// the second.
#[test]
fn a_selection_across_pieces_reaches_each_one_s_edge_and_fills_the_space_between() {
    let piece = |top: f32| Bounds::new(point(px(0.), px(top)), size(px(100.), px(20.)));
    let span = |top: f32, from: f32, to: f32| Span {
        start: point(px(from), px(top)),
        end: point(px(to), px(top)),
        bounds: piece(top),
        row: px(20.),
    };
    let spans = [span(0., 30., 60.), span(24., 0., 0.), span(48., 0., 40.)];

    let corners = |bounds: &Bounds<Pixels>| (bounds.left(), bounds.top(), bounds.right(), bounds.bottom());
    let painted: Vec<(Pixels, Pixels, Pixels, Pixels)> = boxes(&spans).iter().map(corners).collect();

    let expected =
        [(30., 0., 100., 20.), (0., 20., 100., 24.), (0., 24., 100., 44.), (0., 44., 100., 48.), (0., 48., 40., 68.)];
    assert_eq!(painted, expected.map(|(left, top, right, bottom)| (px(left), px(top), px(right), px(bottom))));
}

/// What a command printed, `one`, a blank line and `three`, laid out as `rows::tools::block` lays it out: a piece a
/// line, each with the blank lines after it. A selection from part way into the first line to the start of the third
/// is painted to the first's edge, through the blank line, and down to the third, with none of its text. One within a
/// piece is painted only there.
#[gpui_kit::test]
fn what_s_selected_is_painted_on_the_pieces_it_reaches_and_no_others(cx: &mut TestAppContext) {
    let said = "one\n\nthree";
    let (pieces, layouts) = in_pieces(cx.add_empty_window(), &["one\n", "three"], 4.);
    let (first, last, row) = (layouts[0].bounds(), layouts[1].bounds(), layouts[1].line_height());
    assert!(pieces.lay_out(said));

    let into_the_last = pieces.selected_boxes(1..5, said);
    assert_eq!(into_the_last[0].origin, layouts[0].position_for_index(1).expect("it's laid out"), "from its start");
    assert_eq!(into_the_last[0].right(), first.right(), "to the first's edge");
    let lowest = into_the_last.iter().map(Bounds::bottom).fold(Pixels::ZERO, Pixels::max);
    assert_eq!(lowest, last.top() + row, "down to the last, and along its first row");
    let on_the_last = |bounds: &&Bounds<Pixels>| bounds.top() >= last.top() && bounds.size.width > Pixels::ZERO;
    assert_eq!(into_the_last.iter().filter(on_the_last).count(), 0, "with none of its text: {into_the_last:?}");

    let within_the_last = pieces.selected_boxes(7..9, said);
    assert!(!within_the_last.is_empty() && within_the_last.iter().all(|bounds| bounds.top() >= last.top()));
    let within_the_first = pieces.selected_boxes(0..2, said);
    assert!(!within_the_first.is_empty() && within_the_first.iter().all(|bounds| bounds.bottom() <= first.top() + row));
}

/// A selection from where a line wraps is painted from the start of the row it wraps onto, rather than from the end of
/// the row before, where GPUI puts that place.
#[gpui_kit::test]
fn a_selection_from_where_a_line_wraps_starts_on_the_row_it_wraps_onto(cx: &mut TestAppContext) {
    let text = "aaaa bbbb";
    let window = cx.add_empty_window();
    let wide = |width: Pixels, window: &mut VisualTestContext| {
        let shown = StyledText::new(text);
        let layout = shown.layout().clone();
        window.draw(point(px(0.), px(0.)), size(width, px(100.)), |_, _| shown.into_element());
        layout
    };
    let one_row = wide(px(1000.), window);
    let width = one_row.position_for_index(1).unwrap().x - one_row.position_for_index(0).unwrap().x;
    // Room for `aaaa ` and not the rest.
    let layout = wide(width * 6.5, window);
    let (bounds, row) = (layout.bounds(), layout.line_height());
    assert_eq!(bounds.size.height, row * 2., "it wraps onto a second row");

    let painted = Pieces::whole(layout.clone()).selected_boxes(5..9, text);

    assert_eq!(painted.len(), 1, "one box, on the second row: {painted:?}");
    assert_eq!(painted[0].origin, point(bounds.left(), bounds.top() + row));
}
