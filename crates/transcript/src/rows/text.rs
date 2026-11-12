//! How a row draws a text written in the chat, by the person or the agent: once it's recorded in a row of its own,
//! from the kit's blocks, through `crate::prose`, and otherwise with the kit's text view. Either way its view is known
//! by [`find::view_id`], from its first word to its record, so what the view read while the text was written or sent
//! carries over to the record until its parse is in.

use gpui_kit::AnyElement;
use gpui_kit::component::text::TextView;
use protocol::event::LinkedImage;

use super::Shared;
use crate::find::{self, FindLook, Part, TextId};
use crate::item_key::ItemKey;
use crate::long_texts::LongTexts;
use crate::prose::{Images, TextAt, Written};

/// The recorded text `id` at `at`, in a row of its own, drawn from the kit's blocks, with what's found in it as `find`
/// says, and with the images the hub kept of those it points to, if it can point to any. Until it's parsed, it's the
/// view it was written or sent into, if that's still kept, so it doesn't go blank between.
pub(super) fn recorded(
    at: TextAt,
    id: &TextId,
    text: &str,
    images: Option<&[LinkedImage]>,
    find: &FindLook,
    shared: &Shared,
) -> AnyElement {
    let resolver = || images.map(|images| shared.linked.resolver(at.seq, images));
    let meanwhile = || {
        let view = shared.texts.kept(find::view_id(id), text)?;
        let view = match resolver() {
            Some(resolver) => view.image_source(resolver),
            None => view,
        };
        Some(find::markdown_view(view, id, find, None))
    };
    let images = resolver().map(|resolver| -> Images { Box::new(resolver) });
    shared.prose.draw(&shared.selection, &Written::found(at, id, find, text), images, meanwhile)
}

/// Text the agent wrote under `key`, in the kit's text view, as it streams, and once it's recorded in a call's row.
pub(super) fn agent_text(texts: &LongTexts, key: &ItemKey, text: &str) -> TextView {
    texts.markdown(find::view_id(&(key.clone(), Part::Text)), text)
}
