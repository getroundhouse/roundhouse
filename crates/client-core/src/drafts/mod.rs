//! Keeping what's typed and not sent on the hub, so it's there after the app restarts, and in every client: what's
//! typed and attached in each chat's composer, what's chosen and typed in answer to each chat's question, the comment
//! being written on each workspace's diff, and what's in the new-workspace dialog, files included. A client hands over
//! each change as it's made, and a thread of its own keeps the newest of each, one call at a time, so they reach the
//! hub in the order they were made. Each file goes before any draft that names it, since the hub leaves out a file it
//! hasn't got, and one let go of goes after the draft that stopped naming it, since the hub keeps any a draft names.
//! What a hub can't take, because the connection went or it doesn't keep that kind, waits for the next connection. The
//! count of what hasn't reached the hub, for each draft, says what quitting would lose, and quitting
//! waits for it with [`Drafts::flushed`], which needs no other thread. The same count, and the last copy of what's
//! there that this client and the hub agree on, say whether what's on the hub was made somewhere else, such as in
//! another client, as [`Drafts::from_elsewhere`] says.

#[cfg(test)]
use crate::{Client, FileToSend};
#[cfg(test)]
use protocol::model::{Draft, DraftToKeep, NewWorkspaceDraftToKeep, Rev, Session, SessionId};

mod keeper;
mod read_back;
#[cfg(test)]
mod tests;

pub use read_back::{CameBack, NotReadBack, ReadBack};

mod draft_state;
pub(crate) use draft_state::PAUSE;
use draft_state::{Agreed, Counts, Request, Said, agree};
pub use draft_state::{DraftChange, DraftKey, Drafts, FilesFor, KeptDraft};
