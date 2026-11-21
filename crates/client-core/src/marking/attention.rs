//! Whether a chat or a workspace needs the person to look at it: a reply nobody has read, or an agent waiting on them.
//! Every client's names stand out for either.

use protocol::model::{Session, SessionStatus, Workspace, WorkspaceStatus};

/// Whether an open chat is waiting on the person, for an answer or an approval. It needs a look as a reply nobody has
/// read does, so a name stands out for either.
pub fn waiting_on_you<'a>(chats: impl IntoIterator<Item = &'a Session>) -> bool {
    chats.into_iter().any(|chat| chat.is_open() && chat.status == SessionStatus::WaitingApproval)
}

/// Whether a workspace needs a look, so its name stands out: one of its open `chats` has a reply nobody has read, as
/// `unread` says of each, or is waiting on the person. An archived one's unread replies count again once it's back.
pub fn needs_a_look(workspace: &Workspace, chats: &[Session], unread: impl Fn(&Session) -> bool) -> bool {
    let has_unread = workspace.status != WorkspaceStatus::Archived && chats.iter().any(|c| c.is_open() && unread(c));
    has_unread || waiting_on_you(chats)
}
