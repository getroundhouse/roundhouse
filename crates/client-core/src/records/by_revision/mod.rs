//! What a client keeps by revision, by one rule for records and observations alike: of two copies of one, the newer
//! is kept, whichever arrives first, a removal drops an older copy, and a list of everything drops what it lacks,
//! unless the copy held is newer than the list.

#[cfg(test)]
mod tests;

mod held;
mod kinds;
pub(super) use held::{ByRevision, Revised};
