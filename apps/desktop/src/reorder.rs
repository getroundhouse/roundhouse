//! A list of workspaces whose order can change while someone is using it, as the sidebar's and Home's can. Each row
//! that moves slides from where it was to its new place, rather than jumping, so it's plain where it went, and the
//! order holds still while someone is using the list, so a row doesn't move out from under a click. A list keeps a
//! [`Places`] and a [`Hold`] of its own from one frame to the next, and draws each row with [`offset_by`].

mod hold;
mod places;

use gpui_kit::{AnyElement, IntoElement, ParentElement as _, Pixels, Styled as _, div};

#[cfg(feature = "testing")]
pub(crate) use hold::HOLDS_FOR;
pub(crate) use hold::{Hold, hears_the_pointer, hold_while_open, keep_held};
pub(crate) use places::Places;

/// A row, drawn `offset` from the place the list lays it out in, as it slides there. The list lays each row out on its
/// own, which takes no offset, so the row is offset inside a box that fills its place.
pub(crate) fn offset_by(row: impl IntoElement, offset: Pixels) -> AnyElement {
    div().size_full().child(div().size_full().relative().top(offset).child(row)).into_any_element()
}
