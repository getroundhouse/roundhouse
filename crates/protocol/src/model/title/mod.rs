//! What a workspace or a chat is called on screen, and where that came from.

#[cfg(test)]
mod laws;

mod text;
#[cfg(test)]
use text::breaks_line;
pub use text::{Title, TitleError, TitleSource};
