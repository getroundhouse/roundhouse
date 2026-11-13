use gpui_kit::{Bounds, TestAppContext, point, px};

use super::test_layouts::in_pieces;
use super::{End, Geometry, Selection};
use crate::texts::{TextAt, TextKey, Texts};

/// A text that's selected keeps what's selected of it as it moves, and is copied from where it is now.
#[gpui_kit::test]
fn a_selection_stays_in_a_text_that_moves(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let focus = cx.update(|cx| cx.focus_handle());
    let selection = Selection::new(texts.clone(), focus);
    let (reply, written, recorded) = (TextKey::named("reply"), TextAt::only(1, 0), TextAt::only(7, 0));
    texts.drawn_as(&reply, written, "Hello world");
    let end = |byte| End { text: reply.clone(), leaf: 0, byte };
    selection.inner.borrow_mut().set(Some((end(0), end(5))));
    texts.new_frame();

    texts.drawn_as(&reply, recorded, "Hello world");

    assert_eq!(selection.text(), "Hello");
    assert_eq!(selection.held(), Some(recorded..=recorded), "and it's where it is now that's kept for a copy");
}

/// A press on a text selects nothing yet, but its text is kept, so a shift-click can extend from it once its row's
/// been let go of.
#[gpui_kit::test]
fn the_text_a_press_was_on_is_kept_with_nothing_selected(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let focus = cx.update(|cx| cx.focus_handle());
    let selection = Selection::new(texts.clone(), focus);
    let (reply, at) = (TextKey::named("reply"), TextAt::only(3, 0));
    texts.drawn_as(&reply, at, "Hello world");
    let pressed = End { text: reply, leaf: 0, byte: 2 };

    selection.inner.borrow_mut().set(Some((pressed.clone(), pressed)));

    assert_eq!(selection.text(), "", "nothing's selected");
    assert_eq!(selection.held(), Some(at..=at), "but the text's kept");
}

/// A plain text that shows only the space between two of its lines, where an area cuts it short or at the list's
/// edge, shows none of its leaves, though the space is within its bounds, so nothing finds a place in a line of it
/// that doesn't show. Once a line shows, its leaf does.
#[gpui_kit::test]
fn a_text_that_shows_only_the_space_between_its_lines_shows_none_of_its_leaves(cx: &mut TestAppContext) {
    let selection = Selection::new(Texts::alone(), cx.update(|cx| cx.focus_handle()));
    let (pieces, layouts) = in_pieces(cx.add_empty_window(), &["abc", "de"], 6.);
    let (above, top) = (layouts[0].bounds().bottom(), layouts[1].bounds().top());
    let space = Bounds::from_corners(point(px(0.), above + px(1.)), point(px(200.), top - px(1.)));
    let into_the_second = Bounds::from_corners(space.origin, point(px(200.), top + px(1.)));
    let (output, at) = (TextKey::named("output"), TextAt::only(1, 0));
    // How many of its leaves show, drawn in a frame of their own where what shows of it is `shows`.
    let draw = |shows| {
        selection.new_frame();
        selection.drawn(output.clone(), at, shows, None, [(0, Geometry::Plain(pieces.clone()))]);
        selection.inner.borrow().drawn[0].leaves.len()
    };

    assert!(pieces.bounds().intersects(&space), "the space is within the text's bounds");
    assert_eq!(draw(space), 0, "but none of its leaves shows there");
    assert_eq!(draw(into_the_second), 1, "and its leaf shows once the second line does");
}
