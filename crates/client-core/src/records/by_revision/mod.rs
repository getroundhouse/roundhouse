//! What a client keeps by revision, by one rule for records and observations, and every level they're sent at: of two
//! copies of one, the newer is kept, whichever arrives first, a removal drops an older copy, and a level's list drops
//! what it lacks of that level, unless the copy held is newer than the list, and says what any copy or removal of that
//! level no newer than it says. Revisions are compared on one connection alone, so the first list of a level on a
//! connection replaces whatever earlier connections left of that level.

#[cfg(test)]
mod tests;

mod held;
mod kinds;
pub(super) use held::{ByRevision, Revised};
