//! How a row draws a text written in the chat, by the person or the agent, from the kit's blocks, through
//! `crate::texts`: in a row of its own, at the end of the chat, or nested in a call's row. Wherever it is, it's
//! known by [`crate::find::view_id`], from its first word to its record, so what it was parsed as while it was written or
//! sent carries over to its record, where `crate::texts` parses a reply whole if it was parsed from near its end. And
//! where a call's texts go among the others, its plain text too, such as a command and what it printed.

use gpui_kit::{AnyElement, ElementId};
use protocol::event::LinkedImage;

use super::tools::Toggle;
use super::{InRow, Shared};
use crate::find::Part;
use crate::texts::{Among, Of, Place, TextAt, Written};

/// A recorded text, the `part` of the item the block `toggle` draws, with the images the hub kept of those it points
/// to, if it can point to any, and with its matches, in the area `area` if one scrolls it, at the [`place`] its part
/// gives it.
pub(super) fn written(
    toggle: &Toggle,
    part: Part,
    text: &str,
    area: Option<ElementId>,
    images: Option<&[LinkedImage]>,
) -> AnyElement {
    let (id, find, shared) = ((toggle.key.clone(), part), toggle.find, toggle.shared);
    from_blocks(&Written { area, ..Written::found(place(toggle, part), &id, find, text) }, images, shared)
}

/// Where the `part` of the item the block `toggle` draws goes. In a row of its own, its part places it among the texts
/// its block draws. Nested in a call's row, it's the next text that call nests, so a row makes its nested texts in the
/// order it shows them.
pub(in crate::rows) fn place(toggle: &Toggle, part: Part) -> Place {
    match toggle.in_row {
        InRow::Own(block) => Place::At(TextAt { of: Of::Event(toggle.seq), block, among: among(part) }),
        InRow::Nested(block) => Place::NextNested { seq: toggle.seq, block },
    }
}

/// Where a text goes among those its block draws, by what it is: a reply's text and a thought are all their blocks
/// draw, and anything else is one of a call's own. That's before what the call nests if the call shows it first, as
/// it does what went wrong, under its line, and what a subagent was asked, and after it otherwise, as its report is.
fn among(part: Part) -> Among {
    if matches!(part, Part::Text | Part::Thinking) {
        Among::Only
    } else if matches!(part, Part::Error | Part::Asked) {
        Among::Before(part)
    } else {
        Among::After(part)
    }
}

/// The text `written`, drawn from the kit's blocks, with the images the hub kept of those it points to, if it's in an
/// event's row and can point to any, which a click opens over the window.
pub(super) fn from_blocks(written: &Written<'_>, images: Option<&[LinkedImage]>, shared: &Shared) -> AnyElement {
    let images = (written.at.seq()).zip(images).map(|(seq, images)| shared.linked.images(seq, images));
    shared.texts.draw(&shared.selection, written, images)
}

#[cfg(test)]
mod tests {
    use super::{Among, Part, among};

    /// As a call shows them: what went wrong, under its line, what a subagent was asked, what the call nests, and then
    /// what it opens to of its own, such as a command, what it printed, and a subagent's report.
    #[test]
    fn a_call_s_texts_go_in_the_order_it_shows_them() {
        let places = [
            among(Part::Error),
            among(Part::Asked),
            Among::Nested(0),
            Among::Nested(usize::MAX),
            among(Part::Command),
            among(Part::Output),
            among(Part::Report),
        ];

        assert!(places.is_sorted(), "{places:?}");
        assert!(among(Part::Question) < among(Part::Answered), "and what was answered to a question goes after it");
        assert_eq!(among(Part::Text), Among::Only, "and a reply's is its block's only text");
    }
}
