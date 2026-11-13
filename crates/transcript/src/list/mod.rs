//! The list a transcript's rows are drawn in: only the rows on screen, keeping to the latest row until someone
//! scrolls away, with a scrollbar and a button that scrolls back down to the latest row.
//!
//! It's the transcript's own, rather than GPUI's list, to hold to the four rules `docs/ui.md` gives. It draws only
//! what's on screen, so a long session costs what a short one does. It keeps its place as a row and how far into
//! it, so nothing that changes a height out of sight moves what's in sight, and keeps a row in sight where it is, as
//! [`Anchor`] says, so a row in sight that changes height by itself moves nothing else on screen. And it keeps each
//! row's height as it was last measured, as [`Heights`] says, and guesses at the rest, so the scrollbar goes by the
//! whole session without any of it being measured for that. GPUI's list measures every row or only those on screen,
//! and forgets every height when its width changes.
//!
//! The kit's scrollbar and its bounce at the ends work on anything that says how far it's scrolled and how tall it
//! is, which [`TranscriptList`] does for them.

mod element;
mod heights;
mod layout;
mod state;

#[cfg(doc)]
use heights::Heights;
#[cfg(doc)]
use layout::Anchor;
pub use state::Place;

mod transcript_list;

pub use transcript_list::TranscriptList;
