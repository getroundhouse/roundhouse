//! What a client asks a hub to do.
//!
//! Each request that changes something is safe to send again after a lost reply: it either has the
//! same effect however often it's applied, or carries an ID the client chose, which the hub records
//! once. Each says which.

use serde::{Deserialize, Serialize};

use crate::attachment::{AttachmentBytes, MediaType};
use crate::event::ApprovalDecision;
use crate::model::{
    AgentKind, ApprovalId, AttachmentId, ClientMessageId, ClientRequestId, CommentId, DiffSide, RepoId, Seq, SessionId,
    WorkspaceId,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ClientRequest {
    /// Must come first on a connection.
    Hello {
        protocol_version: u32,
        client: String,
    },
    /// Works without a hello, and so does `HubShutdown`, so a client of any version can see a hub and
    /// stop it. Neither can change form, even in a real break.
    HubStatus,
    /// Stops the hub and its workers. Agents are stopped and resume on the next prompt.
    HubShutdown,

    /// Adding the same repository again returns the first record.
    RepoAdd {
        path: String,
    },
    RepoList,
    /// Every repository, workspace, session and comment, read at one moment, with the revision it
    /// was read at. Follow the feed first, then list, and keep whichever copy of a record is newer.
    RecordsList,

    /// Generates a name if none is given. Uses the repository's default branch if no base is given.
    /// Sending the same `client_request_id` again returns the workspace it made, in whatever state it
    /// has reached, rather than making another.
    WorkspaceCreate {
        repo_id: RepoId,
        name: Option<String>,
        base_ref: Option<String>,
        #[serde(default)]
        client_request_id: Option<ClientRequestId>,
    },
    WorkspaceList {
        include_archived: bool,
    },
    /// Fails if work would be lost or the archive script fails, unless `force` is set. The branch is kept.
    /// Archiving it again is refused, changing nothing.
    WorkspaceArchive {
        id: WorkspaceId,
        force: bool,
    },
    /// Restoring one that isn't archived is refused, changing nothing.
    WorkspaceRestore {
        id: WorkspaceId,
    },
    /// Sets the title someone typed, which nothing generated replaces. It's tidied as
    /// [`Title::typed`](crate::model::Title::typed) says, and refused if that leaves nothing or too much. Sent
    /// again, it sets the same again.
    WorkspaceRename {
        id: WorkspaceId,
        title: String,
    },
    WorkspaceStatus {
        id: WorkspaceId,
    },
    WorkspaceDiff {
        id: WorkspaceId,
    },
    /// The end of the setup log, which the worker keeps.
    WorkspaceSetupLog {
        id: WorkspaceId,
    },
    /// Copies local files and runs the setup script again. Refused while setup is running. Sent again
    /// once it has finished, it runs setup again, since it carries no ID.
    WorkspaceRerunSetup {
        id: WorkspaceId,
    },
    /// The pull request for this workspace's branch. The reply is `None` if there isn't one, or if
    /// GitHub's CLI isn't available.
    WorkspacePullRequest {
        id: WorkspaceId,
    },
    /// Sending the same `client_request_id` again returns the comment it made, unless it's been deleted
    /// since, which forgets the ID.
    CommentAdd {
        workspace_id: WorkspaceId,
        path: String,
        side: DiffSide,
        line: u32,
        text: String,
        #[serde(default)]
        client_request_id: Option<ClientRequestId>,
    },
    CommentList {
        workspace_id: WorkspaceId,
    },
    /// Replaces a comment's text, keeping its place in the diff. Sending it again sets the same text.
    CommentEdit {
        id: CommentId,
        text: String,
    },
    /// Deleting a comment that's already gone does nothing.
    CommentDelete {
        id: CommentId,
    },

    /// Sending the same `client_request_id` again returns the session it made.
    SessionCreate {
        workspace_id: WorkspaceId,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
        #[serde(default)]
        client_request_id: Option<ClientRequestId>,
    },
    /// The workspace's newest open session, or a new one if it has none. Clients opening a workspace
    /// at the same moment end up in the same session, rather than one each.
    SessionEnsure {
        workspace_id: WorkspaceId,
        agent: AgentKind,
    },
    SessionList {
        workspace_id: WorkspaceId,
    },
    SessionGet {
        id: SessionId,
    },
    /// Reads events from `from_seq` on. Returns at least one event if any exist, and stops at
    /// `limit` events or about `max_bytes`, so a huge event can't produce an unbounded reply.
    SessionRead {
        id: SessionId,
        from_seq: Seq,
        limit: u32,
        max_bytes: Option<u64>,
        #[serde(default)]
        include_raw: bool,
    },
    /// Sends every event from `from_seq` on, then keeps streaming as new ones are recorded. To
    /// catch up after reconnecting, subscribe from one past the last `seq` received. The client
    /// picks `subscription_id`, because events can arrive before the reply does.
    SessionSubscribe {
        id: SessionId,
        from_seq: Seq,
        subscription_id: u64,
    },
    /// Works for any kind of subscription.
    SessionUnsubscribe {
        subscription_id: u64,
    },
    /// Sends a [`super::HubNotice`] whenever a record changes. Subscribe, then list with `RecordsList`, and
    /// keep whichever copy of each record has the newer revision. List again on `Resync`.
    FeedSubscribe {
        subscription_id: u64,
    },
    /// Resolves once the prompt is recorded. Sending the same `client_msg_id` twice records it once,
    /// so retrying after a disconnect can't send it twice. A retry only records: if the hub stopped
    /// before delivering the original, the session shows as interrupted, and the prompt needs sending
    /// again with a new ID. Each of `attachments` has to have been added to this session first, and a
    /// prompt with any can have no text.
    SessionPrompt {
        id: SessionId,
        text: String,
        client_msg_id: Option<ClientMessageId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentId>,
    },
    /// Adds a file to a session, for a prompt to carry. Sending the same `id` again returns the attachment it
    /// made, whatever else it says. Refused if the file is too big, or isn't the kind it says, as
    /// [`MediaType::check`] decides. A hub older than attachments refuses it as unsupported, so a client finds
    /// out before it sends the prompt.
    AttachmentAdd {
        session_id: SessionId,
        id: AttachmentId,
        name: String,
        media_type: MediaType,
        data: AttachmentBytes,
    },
    /// An image's bytes, for drawing it. Only the session it was added to has it, and once its prompt has gone,
    /// the hub keeps only images, so any other file is not found.
    AttachmentRead {
        session_id: SessionId,
        id: AttachmentId,
    },
    /// Answering again is refused, changing nothing.
    SessionApprove {
        id: SessionId,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
    /// Cancelling when nothing is running does nothing. Sent again once a new turn has started, it cancels
    /// that one too, since it carries no ID.
    SessionCancel {
        id: SessionId,
    },
    /// Changes the model or approval mode, on the running agent too if there is one. `None` leaves
    /// that setting as it is. Fails, changing nothing, if the agent refuses. Sent again, it sets the same
    /// again.
    SessionConfigure {
        id: SessionId,
        model: Option<String>,
        permission_mode: Option<String>,
    },
    /// Stops its agent and hides it from the tabs. The history stays, and it can be reopened. Closing
    /// it again leaves it closed.
    SessionClose {
        id: SessionId,
    },
    /// Reopening an open session leaves it open.
    SessionReopen {
        id: SessionId,
    },
    /// Marks the session read through `through_seq`, so only a reply after it is unread. It never reaches past the
    /// newest event, so a reply still to come stays unread. Marking read what already is changes nothing, so sending
    /// it again does nothing more.
    SessionMarkRead {
        id: SessionId,
        through_seq: Seq,
    },
    /// Marks the newest reply unread again. With no reply, or one that's already unread, it changes nothing, so sending
    /// it again does nothing more.
    SessionMarkUnread {
        id: SessionId,
    },
    /// Sets the title someone typed, as [`ClientRequest::WorkspaceRename`] does for a workspace.
    SessionRename {
        id: SessionId,
        title: String,
    },
    /// Any request this build doesn't know, from a client newer than it. A hub answers with
    /// [`super::ErrorCode::Unsupported`] and carries on.
    #[serde(other)]
    Unknown,
}
