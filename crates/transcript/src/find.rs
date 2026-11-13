//! What drawing needs to find text in a chat, as `docs/spec/chats.md` says: which of an item's texts each is, where
//! the needle is in each as it's drawn, and which match is the one being shown and where it was drawn. Finding itself,
//! from the bar to what's counted, is the transcript's, in `crate::transcript::find`.

mod look;
pub(crate) mod markdown;
mod marked;
pub(crate) mod reveal;

pub(crate) use look::{FindLook, Marks, Part, Spot, TextId, behind, view_id};
pub(crate) use marked::{marked, marked_with_layout, styled};
