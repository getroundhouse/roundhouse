//! What's in the new-workspace dialog and not made yet, which is one record, and the files waiting with it. No session
//! exists yet to add them to, so they're kept apart from every session's. What a file is never changes once it's added.
//! Its bytes aren't here, but in a file of their own, which `attachment_files` keeps.

#[cfg(test)]
use super::{lock, now_ms, text_of};
#[cfg(test)]
use protocol::attachment::Attachment;
#[cfg(test)]
use protocol::model::{AttachmentId, NewWorkspaceDraft, NewWorkspaceDraftToKeep, Rev};

#[cfg(test)]
mod tests;

mod draft_row;

pub(super) use draft_row::{new_workspace_draft, read_draft_file};
