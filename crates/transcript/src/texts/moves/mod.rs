//! Where each text is in the chat now. A text is kept by what it's known by, which stays the same as it moves: as a
//! reply does from being written to its record, or a prompt from waiting at the end of the chat to the row where the
//! agent took it. So a text drawn somewhere new is drawn at once from what it was parsed as, a parse on another thread
//! lands wherever the text is when it's in, and a selection that names it, as `crate::selection` does, stays in it.
//! Where it is orders it among the others. One joined as it went on is parsed whole the first time it's drawn away
//! from the end of the chat, as `parsing` says.
//!
//! A text drawn where another was takes its place, as a prompt waiting at the end of the chat does when the one before
//! it is taken, since the list draws the end before the row where the agent took that one. The other has no place
//! until it's drawn again, but keeps where it was last drawn, which says whether it's still loaded, so it's kept, with
//! what it was parsed as, as long as it would have been there.
//!
//! The end of the chat is always there. It keeps what it drew and doesn't draw now, such as a reply the agent has
//! finished writing, until it's drawn in the row of its record, which can come after, since the hub sends the end of
//! what's written as soon as it comes, and the record once it's stored. A reply the agent was stopped from writing
//! before it was recorded never is, so the end keeps only the few of those it drew most recently. It can't let go of
//! them once the turn's over instead, since the turn's end can come with the record, before the record's row is drawn,
//! and the reply would be parsed again from nothing there.

#[cfg(test)]
mod tests;

mod drawing;

pub(super) use drawing::Places;
