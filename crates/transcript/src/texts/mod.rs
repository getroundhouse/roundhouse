//! The chat's texts whose selection the transcript keeps, rather than the window, so what's selected of them, and
//! what's found in them, outlasts their rows being drawn. They're the chat's Markdown, drawn from the kit's blocks
//! rather than by the kit's text view: the agent's replies and thoughts, from when it starts writing them, prompts,
//! from when they're sent, a plan to approve, a compaction's summary, the prose a call shows of its own, such as a
//! subagent's report, what a call's row nests, such as a subagent's words, and what the subagent tab says its subagent
//! was asked and reported, above and below its rows. They're also the plain text a call shows, such as a command and
//! what it printed, which `plain` draws. `docs/ui.md` has why.
//!
//! The kit parses the Markdown and draws each block as its text view would, through its `editor_host`. Each
//! paragraph, heading, code block and table cell is a leaf, keyed by its place in the text. The kit says where it drew
//! each leaf, so the transcript can find the character under the pointer, and paints the ranges it's given behind it,
//! which is how what's selected and what's found are drawn. `crate::selection` has the rest. Plain text isn't parsed.
//! It's one leaf, what it says as find searches it, however it's laid out, such as a line at a time, and it's kept, as
//! what it says, as any other text is.
//!
//! A text is parsed the first time its row is drawn, which is soon after the row loads, since the list measures
//! loaded rows out of sight. What it shows is kept while its row is loaded, or it's selected, for a copy, and the
//! subagent tab's head and foot while the tab is open. The blocks it's drawn from are kept for a long one, and for a
//! short one only while it's among those drawn most recently, as `budget` says, and a short one is parsed again when
//! it's next drawn. A text at the end of the chat that goes on from what it was parsed as is parsed again from near
//! its end, keeping the blocks before, and is parsed whole once it's drawn anywhere else, since the two can differ.
//! When more than [`AT_ONCE`] is to be parsed, it's parsed on another thread, as `parsing` says, and draws what it was
//! parsed as before until it's in, or nothing if it wasn't, and then its row is measured again. A text is kept by what
//! it's known by, which is the same from a reply's first word to its record, and from a prompt being sent to the row
//! where the agent took it, so it's drawn from what it was parsed as once it's moved there, as `moves` says, and
//! doesn't go blank between.
//!
//! A row draws its event's texts through [`Texts::drawing`], as the subagent tab draws its head and foot, which hides
//! each text it no longer draws, such as a thought that's been closed, so a copy has only what's shown. A hidden text
//! is kept, as any other is, so it's drawn again at once when it's shown again.

pub(crate) use at::{Among, Of, Place, TextAt};
use parse::ImagesAre;
pub(crate) use parse::{ATOM, Parsed, leaves};
pub(crate) use plain::Plain;

mod at;
mod budget;
mod element;
mod moves;
mod parse;
mod parsing;
mod plain;

#[cfg(test)]
mod tests;

mod cache;
use cache::{AT_ONCE, Store, Text};
pub(crate) use cache::{Leaves, TextKey, Texts, Written};
