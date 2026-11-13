use gpui_kit::TestAppContext;

use super::{End, Selection};
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
