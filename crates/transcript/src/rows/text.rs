//! How a row draws a text written in the chat, by the person or the agent: once it's recorded in a row of its own,
//! from the kit's blocks, through `crate::prose`, and otherwise with the kit's text view. Either way its view is known
//! by [`find::view_id`], from its first word to its record, so what the view read while the text was written or sent
//! carries over to the record until its parse is in.

use gpui_kit::{AnyElement, ElementId};
use protocol::event::LinkedImage;

use super::Shared;
use super::tools::Toggle;
use crate::find::{self, Part};
use crate::prose::{Among, Images, Of, TextAt, Written};

/// A recorded text, the `part` of the item the block `toggle` draws, as the row it's in draws it, with the images the
/// hub kept of those it points to, if it can point to any, and with its matches. In a row of its own, the transcript
/// draws it from the kit's blocks, where its part places it among the texts its block draws, in the area `area` if one
/// scrolls it, and selects it itself. In a call's row, as a subagent's words and calls are, it's the kit's text view.
pub(super) fn written(
    toggle: &Toggle,
    part: Part,
    text: &str,
    area: Option<ElementId>,
    images: Option<&[LinkedImage]>,
) -> AnyElement {
    let (id, find, shared) = ((toggle.key.clone(), part), toggle.find, toggle.shared);
    let Some(block) = toggle.in_row.own_block() else {
        let view = shared.texts.markdown(&id, text);
        let view = match images {
            Some(images) => view.image_source(shared.linked.resolver(toggle.seq, images)),
            None => view,
        };
        return find::markdown_view(view, &id, find, area);
    };
    let at = TextAt { of: Of::Event(toggle.seq), block, among: among(part) };
    recorded(&Written { area, ..Written::found(at, &id, find, text) }, images, shared)
}

/// Where a text goes among those its block draws, by what it is: a reply's text and a thought are all their blocks
/// draw, and anything else is one of a call's own, such as what a subagent was asked and its report.
fn among(part: Part) -> Among {
    if matches!(part, Part::Text | Part::Thinking) { Among::Only } else { Among::Call(part) }
}

/// The recorded text `written`, drawn from the kit's blocks, with the images the hub kept of those it points to, if it
/// can point to any. Until it's parsed, it's the view it was written or sent into, if that's still kept and find looks
/// in it, so it doesn't go blank between.
pub(super) fn recorded(written: &Written<'_>, images: Option<&[LinkedImage]>, shared: &Shared) -> AnyElement {
    let resolver = || Some(shared.linked.resolver(written.at.seq()?, images?));
    let meanwhile = || {
        let (id, find) = written.found?;
        let view = shared.texts.kept(id, written.text)?;
        let view = match resolver() {
            Some(resolver) => view.image_source(resolver),
            None => view,
        };
        Some(find::markdown_view(view, id, find, written.area.clone()))
    };
    let images = resolver().map(|resolver| -> Images { Box::new(resolver) });
    shared.prose.draw(&shared.selection, written, images, meanwhile)
}
