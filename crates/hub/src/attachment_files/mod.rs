//! Where the hub keeps the bytes of the files prompts carry, and of the images an agent's tools send: a file each, at
//! `<session>/<attachment>` in the attachments directory, outside the database so copying it before a migration stays
//! quick. The new-workspace dialog's files wait at `workspace-draft.files/<attachment>`, a name no session's can be.
//! Each ID is checked as it's turned into a path, so nothing a client or worker sends can name a file anywhere else.

#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::model::{AttachmentId, SessionId};

#[cfg(test)]
mod tests;

mod storage;

#[cfg(test)]
use storage::NEW_WORKSPACE_DRAFT_DIR;
pub(crate) use storage::{AttachmentFiles, FileFor};
