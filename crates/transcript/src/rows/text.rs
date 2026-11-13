//! How a row draws a text written in the chat, by the person or the agent, from the kit's blocks, through
//! `crate::prose`: in a row of its own, at the end of the chat, or nested in a call's row. Wherever it is, it's
//! known by [`crate::find::view_id`], from its first word to its record, so what it was parsed as while it was written or
//! sent carries over to its record, where `crate::prose` parses a reply whole if it was parsed from near its end.

use gpui_kit::{AnyElement, ElementId};
use protocol::event::LinkedImage;

use super::tools::Toggle;
use super::{InRow, Shared};
use crate::find::Part;
use crate::prose::{Among, Of, Place, TextAt, Written};

/// A recorded text, the `part` of the item the block `toggle` draws, with the images the hub kept of those it points
/// to, if it can point to any, and with its matches, in the area `area` if one scrolls it. In a row of its own, its
/// part places it among the texts its block draws. Nested in a call's row, as a subagent's words and its calls' prose
/// are, it's the next text that call nests, so a row makes its nested texts in the order it shows them.
pub(super) fn written(
    toggle: &Toggle,
    part: Part,
    text: &str,
    area: Option<ElementId>,
    images: Option<&[LinkedImage]>,
) -> AnyElement {
    let (id, find, shared) = ((toggle.key.clone(), part), toggle.find, toggle.shared);
    let at = match toggle.in_row {
        InRow::Own(block) => Place::At(TextAt { of: Of::Event(toggle.seq), block, among: among(part) }),
        InRow::Nested(block) => Place::NextNested { seq: toggle.seq, block },
    };
    from_blocks(&Written { area, ..Written::found(at, &id, find, text) }, images, shared)
}

/// Where a text goes among those its block draws, by what it is: a reply's text and a thought are all their blocks
/// draw, and anything else is one of a call's own, before what the call nests if it's what a subagent was asked, and
/// after it otherwise, as its report is.
fn among(part: Part) -> Among {
    if matches!(part, Part::Text | Part::Thinking) {
        Among::Only
    } else if matches!(part, Part::Asked) {
        Among::Before(part)
    } else {
        Among::After(part)
    }
}

/// The text `written`, drawn from the kit's blocks, with the images the hub kept of those it points to, if it's in an
/// event's row and can point to any, which a click opens over the window.
pub(super) fn from_blocks(written: &Written<'_>, images: Option<&[LinkedImage]>, shared: &Shared) -> AnyElement {
    let images = (written.at.seq()).zip(images).map(|(seq, images)| shared.linked.images(seq, images));
    shared.prose.draw(&shared.selection, written, images)
}

#[cfg(test)]
mod tests {
    use super::{Among, Part, among};

    #[test]
    fn what_a_call_nests_comes_after_what_it_was_asked_and_before_its_report() {
        let places = [among(Part::Asked), Among::Nested(0), Among::Nested(usize::MAX), among(Part::Report)];

        assert!(places.is_sorted(), "{places:?}");
        assert_eq!(among(Part::Text), Among::Only, "and a reply's is its block's only text");
    }
}
