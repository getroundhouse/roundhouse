//! Tests of what `sight` does: a move that waits for what it goes to to be drawn, while what scrolls the texts
//! scrolls a line its way or goes to where it is, and an end out of view brought into view.

use gpui_kit::{Bounds, Pixels, TestAppContext, VisualTestContext, point, px, size};
use protocol::model::Seq;

use super::super::super::test_layouts::{in_pieces, laid_out};
use super::super::super::{Geometry, Pieces, Selection};
use super::super::{Step, Way};
use super::support::{Asked, Scrolling, a_window, frame, place, selecting, step};
use crate::texts::{Of, TextAt, TextKey, Texts};

/// Into another text, a key goes to the nearest one shown, past a hidden one and one with nothing parsed yet, once
/// it's drawn. Until then, as with a drag past the window's edge, what scrolls the texts scrolls a line that way, a
/// line of the end's text, as each frame starts, and a key held down, repeating meanwhile, goes on with that, even once
/// it's scrolled far enough that the end's text isn't drawn, rather than going back to it.
#[gpui_kit::test]
fn into_another_text_a_key_waits_for_it_to_show_and_goes_past_those_not_shown(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let (reply, closed, long, next) =
        (TextKey::named("a"), TextKey::named("b"), TextKey::named("c"), TextKey::named("d"));
    let at = |seq| TextAt::only(Seq::new(seq), 0);
    texts.drawn_as(&reply, at(1), "Signals clear");
    texts.drawn_as(&closed, at(2), "A thought");
    texts.drawing(Of::Event(Seq::new(2)), || {});
    texts.drawn_as(&long, at(3), &"Parsed on another thread. ".repeat(200));
    texts.drawn_as(&next, at(4), "Trains wait");
    selection.inner.borrow_mut().set(Some((place(&reply, 0, 8), place(&reply, 0, 13))));
    let layouts = laid_out(window, &["Signals clear", "Trains wait"], 6.);
    let draw = |shown: &[(&TextKey, u64, usize)]| {
        selection.new_frame();
        for (text, seq, layout) in shown {
            let pieces = Pieces::whole(layouts[*layout].clone());
            let bounds = Geometry::Plain(pieces.clone()).bounds();
            selection.drawn((*text).clone(), at(*seq), bounds, None, [(0, Geometry::Plain(pieces))]);
        }
    };
    let (line, scrolling) = (layouts[0].line_height(), Scrolling::default());

    draw(&[(&reply, 1, 0)]);
    step(&selection, window, Step::Character(Way::On));
    let waits = selection.text();
    let mut asks = vec![frame(&selection, &scrolling, a_window(), || draw(&[(&reply, 1, 0)]))];
    // It's scrolled past the reply, through rows with no text, such as closed calls.
    asks.push(frame(&selection, &scrolling, a_window(), || draw(&[])));
    step(&selection, window, Step::Character(Way::On));
    asks.push(frame(&selection, &scrolling, a_window(), || draw(&[(&next, 4, 1)])));
    let far = selection.inner.borrow().ends.clone().map(|(_, far)| far);
    // And on, in what shows.
    step(&selection, window, Step::Character(Way::On));
    let after = Vec::from_fn(2, |_| frame(&selection, &scrolling, a_window(), || draw(&[(&next, 4, 1)])));

    assert_eq!(waits, "clear", "it waits with the next text not drawn");
    assert_eq!(*scrolling.asked.borrow(), [Asked::Down(line); 3], "while it scrolls a line down at each frame");
    assert_eq!(far, Some(place(&next, 0, 0)), "and once the next shows, the end's there, past the two that aren't");
    assert_eq!(asks, [true; 3], "each frame asking for the next, the last to bring what's selected now into view");
    assert_eq!(after, [false; 2], "and a move within what shows, drawn in view, asks for no frame after");
    assert_eq!(scrolling.asked.borrow().len(), 3, "nor scrolls");
}

/// A selection to the end of a reply, which is drawn, with a text after it that isn't, which a key moving on waits for.
/// Gives the selection, its window, and a line of the reply.
fn at_the_end_of_a_reply(cx: &mut TestAppContext) -> (Selection, &mut VisualTestContext, Pixels) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let (reply, next) = (TextKey::named("reply"), TextKey::named("next"));
    texts.drawn_as(&reply, TextAt::only(Seq::new(1), 0), "Signals clear");
    texts.drawn_as(&next, TextAt::only(Seq::new(2), 0), "Trains wait");
    let layouts = laid_out(window, &["Signals clear"], 6.);
    selection.new_frame();
    let pieces = Pieces::whole(layouts[0].clone());
    let bounds = Geometry::Plain(pieces.clone()).bounds();
    selection.drawn(reply.clone(), TextAt::only(Seq::new(1), 0), bounds, None, [(0, Geometry::Plain(pieces))]);
    selection.inner.borrow_mut().set(Some((place(&reply, 0, 8), place(&reply, 0, 13))));
    (selection, window, layouts[0].line_height())
}

/// A move waits for as long as there's further to scroll, however many lines it takes, as a drag held past the edge
/// scrolls on for as long as it's held, and a key held down goes on with it rather than going back to the end. Once
/// there's nowhere further, such as at the chat's end, it stops.
#[gpui_kit::test]
fn a_move_waits_for_as_long_as_there_s_further_to_scroll_and_then_stops(cx: &mut TestAppContext) {
    let (selection, window, line) = at_the_end_of_a_reply(cx);
    let scrolling = Scrolling::default();

    step(&selection, window, Step::Character(Way::On));
    // Past a stretch of the chat with no text in it, a frame a line.
    for _ in 0..200 {
        frame(&selection, &scrolling, a_window(), || selection.new_frame());
    }
    step(&selection, window, Step::Character(Way::On));
    let asks = frame(&selection, &scrolling, a_window(), || selection.new_frame());
    let last = scrolling.asked.borrow().last().copied();
    let scrolled = scrolling.asked.borrow().len();
    scrolling.at_the_end.set(true);
    let at_the_end = Vec::from_fn(2, |_| frame(&selection, &scrolling, a_window(), || selection.new_frame()));

    assert!(asks, "it goes on asking for frames");
    assert_eq!(last, Some(Asked::Down(line)), "a line on");
    assert_eq!(scrolling.asked.borrow().len(), scrolled + 1, "and at the end, it scrolls once more");
    assert_eq!(at_the_end, [false; 2], "but asks for no frame once that's moved nothing");
    assert_eq!(selection.text(), "clear", "and the end stays");
}

/// A key pressed once the end is out of view, with its text not drawn, has what scrolls the texts go to where the text
/// was drawn first, and moves once it is.
#[gpui_kit::test]
fn a_key_goes_back_to_an_end_that_isn_t_drawn_first(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let (reply, next) = (TextKey::named("reply"), TextKey::named("next"));
    let at = |seq| TextAt::only(Seq::new(seq), 0);
    texts.drawn_as(&reply, at(1), "Signals clear");
    texts.drawn_as(&next, at(2), "Trains wait");
    let layouts = laid_out(window, &["Signals clear", "Trains wait"], 6.);
    let draw = || {
        selection.new_frame();
        for (text, seq, layout) in [(&reply, 1, 0), (&next, 2, 1)] {
            let pieces = Pieces::whole(layouts[layout].clone());
            let bounds = Geometry::Plain(pieces.clone()).bounds();
            selection.drawn(text.clone(), at(seq), bounds, None, [(0, Geometry::Plain(pieces))]);
        }
    };
    selection.inner.borrow_mut().set(Some((place(&reply, 0, 8), place(&reply, 0, 13))));
    let scrolling = Scrolling::default();

    // Scrolled away, nothing's drawn.
    selection.new_frame();
    step(&selection, window, Step::Word(Way::On));
    let waits = selection.text();
    frame(&selection, &scrolling, a_window(), draw);
    frame(&selection, &scrolling, a_window(), draw);

    assert_eq!(waits, "clear", "it waits for the end to be drawn");
    assert_eq!(*scrolling.asked.borrow(), [Asked::Show(Of::Event(Seq::new(1)))], "which it's gone to");
    assert_eq!(selection.text(), "clear\n\nTrains", "and then it moves");
}

/// Where the row the end's text was drawn in shows without it, as a closed thought's does, a key gives up on it a frame
/// after, rather than waiting as long as a row could take to load, and the next press goes to it again.
#[gpui_kit::test]
fn a_key_gives_up_on_an_end_whose_row_shows_without_it(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let (thought, next) = (TextKey::named("thought"), TextKey::named("next"));
    let at = |seq| TextAt::only(Seq::new(seq), 0);
    texts.drawn_as(&thought, at(1), "Signals clear");
    texts.drawn_as(&next, at(2), "Trains wait");
    let layouts = laid_out(window, &["Trains wait"], 6.);
    let draw = || {
        selection.new_frame();
        let pieces = Pieces::whole(layouts[0].clone());
        let bounds = Geometry::Plain(pieces.clone()).bounds();
        selection.drawn(next.clone(), at(2), bounds, None, [(0, Geometry::Plain(pieces))]);
    };
    selection.inner.borrow_mut().set(Some((place(&thought, 0, 8), place(&thought, 0, 13))));
    let scrolling = Scrolling { shows: true, ..Scrolling::default() };

    // Closed, the thought isn't drawn, though its row is.
    draw();
    step(&selection, window, Step::Word(Way::On));
    let asks = Vec::from_fn(3, |_| frame(&selection, &scrolling, a_window(), draw));
    step(&selection, window, Step::Word(Way::On));
    let again = frame(&selection, &scrolling, a_window(), draw);

    let gone_to = Asked::Show(Of::Event(Seq::new(1)));
    assert_eq!(asks, [true, false, false], "it asks for the frame that would draw it, and then no more");
    assert_eq!(*scrolling.asked.borrow(), [gone_to; 2], "and the next press goes to it again");
    assert!(again, "asking for a frame for that");
    assert_eq!(selection.text(), "clear", "and the end stays");
}

/// What scrolls the texts brings the end a key moved into view by as little as it takes, from where it was drawn, as
/// the next frame starts, however many presses came before it, until anything else moves it, such as a press.
#[gpui_kit::test]
fn the_end_a_key_moved_is_brought_into_view_until_something_else_moves_it(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let (pieces, layouts) = in_pieces(window, &["one", "two", "three"], 6.);
    let (output, at) = (TextKey::named("output"), TextAt::only(Seq::new(1), 0));
    texts.said_as(&output, at, "one\ntwo\nthree");
    let draw = || {
        selection.new_frame();
        let bounds = Geometry::Plain(pieces.clone()).bounds();
        selection.drawn(output.clone(), at, bounds, None, [(0, Geometry::Plain(pieces.clone()))]);
    };
    draw();
    selection.inner.borrow_mut().set(Some((place(&output, 0, 0), place(&output, 0, 0))));
    // Showing the first line and a little of the second, so what's below is out of view.
    let first = layouts[0].bounds();
    let shows = Bounds::new(first.origin, size(px(200.), first.size.height + px(4.)));
    let scrolling = Scrolling::default();

    // A key held down can repeat faster than frames come.
    step(&selection, window, Step::Line(Way::On));
    step(&selection, window, Step::Line(Way::On));
    // Nothing really scrolls, so it's still out of view as it's drawn, and asks for another frame to try again.
    let asks = frame(&selection, &scrolling, shows, draw);
    let brought = scrolling.asked.borrow().clone();
    selection.inner.borrow_mut().set(Some((place(&output, 0, 1), place(&output, 0, 1))));
    let after = frame(&selection, &scrolling, shows, draw);

    let by = layouts[2].bounds().bottom() - shows.bottom();
    assert_eq!(brought, [Asked::Down(by)], "to the line two below, at the bottom, as the first frame starts");
    assert!(asks, "with the frame after asked for");
    assert_eq!(*scrolling.asked.borrow(), brought, "and once a press has moved it, it isn't brought into view");
    assert!(!after, "and no more frames are asked for");
}

/// A move that waits for what scrolls the texts to scroll to what it goes to is for the end it was made from, so once
/// anything else moves that end, such as a click, it's let go of, rather than moving the end the click left.
#[gpui_kit::test]
fn a_move_that_waits_is_let_go_of_once_something_else_moves_the_end(cx: &mut TestAppContext) {
    let (selection, window, _) = at_the_end_of_a_reply(cx);
    let scrolling = Scrolling::default();
    let reply = TextKey::named("reply");

    step(&selection, window, Step::Character(Way::On));
    let clicked = (place(&reply, 0, 2), place(&reply, 0, 2));
    selection.inner.borrow_mut().set(Some(clicked.clone()));
    let asks = frame(&selection, &scrolling, a_window(), || {});

    assert!(scrolling.asked.borrow().is_empty() && !asks, "nothing scrolls for it, or asks for a frame");
    assert_eq!(selection.inner.borrow().ends, Some(clicked), "and the click's place stays");
}

/// A step to the start of a place, as a key that steps a view makes, moves the end to the start of the first text
/// there at once if that's drawn, and otherwise has what scrolls the texts go there, and moves once it's drawn.
#[gpui_kit::test]
fn a_step_to_a_place_moves_the_end_to_its_first_text_once_that_is_drawn(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let (reply, prompt) = (TextKey::named("reply"), TextKey::named("prompt"));
    let (replied, asked) = (Of::Event(Seq::new(1)), Of::Event(Seq::new(9)));
    texts.drawn_as(&reply, TextAt::only(Seq::new(1), 0), "Signals clear");
    selection.inner.borrow_mut().set(Some((place(&reply, 0, 13), place(&reply, 0, 7))));
    let far = || selection.inner.borrow().ends.clone().map(|(_, far)| far);
    let scrolling = Scrolling::default();

    window.update(|window, cx| selection.move_to_start_of(TextAt::first_of(replied), window, cx));
    let drawn = far();
    window.update(|window, cx| selection.move_to_start_of(TextAt::first_of(asked), window, cx));
    let waiting = far();
    frame(&selection, &scrolling, a_window(), || texts.drawn_as(&prompt, TextAt::only(Seq::new(9), 0), "Hold it"));

    assert_eq!(drawn, Some(place(&reply, 0, 0)), "to the start of what's drawn there, at once");
    assert_eq!(waiting, drawn, "and to what isn't drawn, not yet");
    assert!(scrolling.asked.borrow().contains(&Asked::Show(asked)), "but once what scrolls the texts goes there");
    assert_eq!(far(), Some(place(&prompt, 0, 0)), "and draws it");
}

/// While a step waits for what scrolls the texts to go to a place, the end it moves from isn't brought into view, so
/// a frame that draws that end out of view, before the place's text is drawn, doesn't scroll back to it.
#[gpui_kit::test]
fn a_step_to_a_place_does_not_bring_back_the_end_it_moves_from(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let (reply, prompt) = (TextKey::named("reply"), TextKey::named("prompt"));
    let at = |seq| TextAt::only(Seq::new(seq), 0);
    texts.drawn_as(&reply, at(1), "Signals clear");
    let layouts = laid_out(window, &["Signals clear"], 6.);
    let draw = || {
        selection.new_frame();
        let pieces = Pieces::whole(layouts[0].clone());
        let bounds = Geometry::Plain(pieces.clone()).bounds();
        selection.drawn(reply.clone(), at(1), bounds, None, [(0, Geometry::Plain(pieces))]);
    };
    selection.inner.borrow_mut().set(Some((place(&reply, 0, 13), place(&reply, 0, 13))));
    let scrolling = Scrolling::default();
    let below = Bounds::new(point(px(0.), px(400.)), size(px(200.), px(200.)));

    // A key moves the end first, which brings it into view.
    draw();
    step(&selection, window, Step::Character(Way::Back));
    frame(&selection, &scrolling, a_window(), draw);
    window.update(|window, cx| selection.move_to_start_of(TextAt::first_of(Of::Event(Seq::new(9))), window, cx));
    frame(&selection, &scrolling, below, draw);
    frame(&selection, &scrolling, below, || {
        draw();
        texts.drawn_as(&prompt, at(9), "Hold it");
    });

    assert_eq!(*scrolling.asked.borrow(), [Asked::Show(Of::Event(Seq::new(9)))], "it goes only to the place");
    assert_eq!(selection.inner.borrow().ends.clone().map(|(_, far)| far), Some(place(&prompt, 0, 0)), "and moves");
}
