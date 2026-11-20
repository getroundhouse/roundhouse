//! The list a transcript's rows are drawn in: only the rows on screen, keeping to the latest row until someone
//! scrolls away, with a scrollbar and a button that scrolls back down to the latest row.
//!
//! It's the transcript's own, rather than GPUI's list, so that a long session costs no more to show than a short
//! one. `docs/decisions/transcript-list-of-its-own.md` says what GPUI's list does instead. This one holds to four
//! rules:
//!
//! - Cost follows what's being looked at, never the length of the session. Only the rows that show in the window are
//!   drawn, and a row out of it is drawn only to find a height that isn't known.
//! - A place is a row and how far into it, not a distance down the session, so nothing that changes a height out of
//!   sight moves what's in sight. A row in sight that changes height by itself moves nothing else on screen either,
//!   since the list keeps one row in sight where it is, as [`Anchor`] says.
//! - A row's height is kept, not worked out again, as [`Heights`] says. It's exact for a row measured at the list's
//!   width and a guess for any other, so the scrollbar goes by the whole session without any of it being measured for
//!   that.
//! - What the list holds and what it has measured change together. It measures a row in the frame that lays the row
//!   out, and a row it hasn't measured counts as a guess. A list that's told of rows in one step and measures them
//!   in a later one takes them to have no height in between, and a turn of the wheel that lands there goes to the
//!   wrong place.
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
