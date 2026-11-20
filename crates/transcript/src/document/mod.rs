//! A Markdown document drawn as the chat draws its Markdown, from the kit's blocks, with a selection it keeps itself,
//! such as a file the file tab shows rendered. So a document can't end the app with a text the kit's parser panics on,
//! and it selects, copies and reads as a reply does. It keeps texts of its own, apart from any chat's, and scrolls
//! them in the chat's column. `docs/decisions/transcript-keeps-its-texts.md` has why the chat's Markdown is drawn this
//! way.

mod changes;
mod frames;
#[cfg(test)]
mod tests;
mod view;

pub use view::Document;
