//! Images the agent's text points to, such as `![The new layout](docs/shot.png)`. One in the workspace is the copy the
//! hub kept when the text came, which the text's block names by the address the text gives it, so a reply keeps what
//! it showed when the agent changes the file again. One written into the text as a `data:` URL is its own bytes.
//! Nothing else is drawn, such as an image on the web, since what an agent writes is untrusted. Each image is decoded
//! off the main thread before it's drawn, and then every row waiting for it is measured again, since the list keeps
//! each row's height, and the image is only its own height once it's decoded.
//!
//! These are kept apart from the pictures a prompt carries, which the transcript keeps by attachment, because the
//! kit's drawing of a text asks for an image from any thread, by its address, so what answers has to be shared across
//! threads. `drawn` has how a text drawn from the kit's blocks draws them, so a click on one opens it over the window.
//!
//! A Mermaid diagram the text writes is drawn as one of these, from what `crate::diagrams` draws of it, so it's drawn,
//! measured again and opened as one the text points to is.

mod drawings;
mod drawn;

#[cfg(test)]
pub(crate) use drawn::taken_at;
pub(crate) use drawn::{diagram_leaf, may_take, to_read};

mod loading;
pub(crate) use drawings::system_fonts;
pub(crate) use loading::{Images, LinkedImages, Waiter};
