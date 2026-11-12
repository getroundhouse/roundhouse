//! What a client asks a hub to do.
//!
//! Each request that changes something is safe to send again after a lost reply: it either has the
//! same effect however often it's applied, or carries an ID the client chose, which the hub records
//! once. Each says which.

use serde::{Deserialize, Serialize};

use super::{drafts, files};
use crate::attachment::{AttachmentBytes, MediaType};
use crate::event::ApprovalDecision;
use crate::model::{
    AgentKind, ApprovalId, AttachmentId, ClientMessageId, ClientRequestId, CommentId, DiffSide, HubSettingsChange,
    Place, RepoId, Seq, SessionId, SessionSettingsChange, ToolUseId, WorkspaceId,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ClientRequest {
    /// Must come first on a connection. It names the newest version of the protocol the client speaks, and the
    /// oldest, which a client from before ranges leaves out, since it speaks only the one.
    Hello {
        protocol_version: u32,
        client: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        oldest_protocol_version: Option<u32>,
    },
    /// Works without a hello, and so does `HubShutdown`, so a client of any version can see a hub and
    /// stop it. Neither can change form, even in a real break.
    HubStatus,
    /// Stops the hub and its workers. Agents are stopped and resume on the next prompt.
    HubShutdown,
    /// Stops the hub as `HubShutdown` does, but only if nothing would notice, as [`HubStatus::is_idle`] says of the
    /// one asking. The hub decides as it stops, so nothing can start between a client seeing it idle and stopping it.
    /// One that's busy refuses with `Conflict`, and carries on. Needs a hello, unlike `HubShutdown`.
    ///
    /// [`HubStatus::is_idle`]: super::HubStatus::is_idle
    HubShutdownIfIdle,
    /// Changes the hub's own settings, which it keeps and every client shows. A setting left out is left as it is. Sent
    /// again, it sets the same again.
    HubConfigure {
        #[serde(flatten)]
        settings: HubSettingsChange,
    },

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
    /// Titles the workspace again from every prompt its chats have had, by a call of its own to a small model, and
    /// replies once that has answered. The new title replaces the one that was there when this was asked, one typed
    /// by hand included, since someone asked, and a title set meanwhile stays. Refused as a conflict unless the
    /// workspace is ready, and until a chat has had a prompt with something typed in it, since there's nothing to
    /// title it from. Sent again, it asks again, since it carries no ID.
    WorkspaceRegenerateTitle {
        id: WorkspaceId,
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
    /// Reads the pull request for this workspace's branch now, and records it on the workspace, which every
    /// client then hears of. Safe to send again, since one asked for while another read waits shares the answer
    /// of the next. The reply is `None` if there isn't one, or if it can't be read here, such as without GitHub's
    /// CLI, a login to it or a GitHub remote, and none was known. When it can't be read just now, such as while
    /// GitHub can't be reached or its hourly limit is used up, the reply is what was known, and an error if
    /// nothing was.
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

    /// Sending the same `client_request_id` again returns the session it made. With no `permission_mode`, its agent
    /// has full access. It's refused for [`PermissionMode::Unknown`](crate::model::PermissionMode::Unknown).
    SessionCreate {
        workspace_id: WorkspaceId,
        agent: AgentKind,
        /// What it starts with, in place of what [`SessionSettings::default`](crate::model::SessionSettings) says.
        #[serde(flatten)]
        options: SessionSettingsChange,
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
    /// Every prompt recorded in the session, oldest first, each as one line, for finding one in a long chat without
    /// reading the whole log. A hub older than this refuses it as unsupported.
    SessionPromptList {
        id: SessionId,
    },
    /// Every compaction of the agent's own conversation that compacted, oldest first, each as one line, for marking
    /// them in a long chat without reading the whole log. A hub older than this refuses it as unsupported.
    SessionCompactionList {
        id: SessionId,
    },
    /// Which of the agent's own events, or those of the subagent `subagent` started, hold `query`, by place, as
    /// [`crate::find::Needle::is_in_event`] says, for finding text in a long chat without reading the whole log. It
    /// only reads. An empty query, or one longer than [`crate::find::Needle::MOST_CHARS`], is refused as invalid, and a
    /// hub older than this refuses it as unsupported.
    SessionFind {
        id: SessionId,
        subagent: Option<ToolUseId>,
        query: String,
    },
    /// Sends every event from `from_seq` on, then keeps streaming as new ones are recorded. To
    /// catch up after reconnecting, subscribe from one past the last `seq` received. The client
    /// picks `subscription_id`, because events can arrive before the reply does.
    SessionSubscribe {
        id: SessionId,
        from_seq: Seq,
        subscription_id: u64,
    },
    /// The records of the session's subagents, which the feed then says change, as `RecordsList` lists every other
    /// record. They're listed by session rather than with the rest, since each holds what it was asked and its report,
    /// in full. A hub older than this refuses it as unsupported.
    SessionSubagents {
        id: SessionId,
    },
    /// Reads the agent's own events, or those of the subagent `subagent` started, from `from_place` on, as
    /// `SessionRead` reads every event. A hub older than this refuses it as unsupported, and a client reads by `seq`.
    SessionReadPlaces {
        id: SessionId,
        subagent: Option<ToolUseId>,
        from_place: Place,
        limit: u32,
        max_bytes: Option<u64>,
        #[serde(default)]
        include_raw: bool,
    },
    /// Follows the agent's own events, or one subagent's, from `from_place` on, as `SessionSubscribe` follows every
    /// event. The tail and deltas are only what that one is writing. To catch up after reconnecting, subscribe from
    /// one past the last place received. A hub older than this refuses it as unsupported.
    SessionSubscribePlaces {
        id: SessionId,
        subagent: Option<ToolUseId>,
        from_place: Place,
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
    /// A file's bytes, for drawing an image, or for putting a draft's file back in the composer. Only the session it
    /// was added to has it, and once its prompt has gone, the hub keeps only images, so any other file is not found.
    AttachmentRead {
        session_id: SessionId,
        id: AttachmentId,
    },
    /// Lets go of a file a client added to a session that's no longer going anywhere, such as one taken off a draft.
    /// The hub keeps one the session's draft, a prompt waiting in its queue or a prompt it was sent still names, and
    /// one put in the worktree, and it never lets go of an image from the agent. One that isn't there changes nothing,
    /// so sending it again does nothing more. Answers with `Ok` either way. A hub older than this refuses it, as
    /// `is_unsupported` says.
    AttachmentRemove {
        session_id: SessionId,
        id: AttachmentId,
    },
    /// Whose account the session's agent runs under, read from the session's worker when asked, and what the hub
    /// has heard of that account's limits. It records nothing, so asking again only reads again, and like any read
    /// from a worker it starts the workspace's worker if none is running. Refused unless the workspace is ready. A
    /// hub older than this refuses it as unsupported.
    ///
    /// Kept for clients from before accounts were records. A client now reads the session's `account` and that
    /// account's record, which the feed announces.
    SessionAccount {
        id: SessionId,
    },
    /// See [`files::WorkspaceFiles`].
    WorkspaceFiles(files::WorkspaceFiles),
    /// See [`files::WorkspaceFolderRead`].
    WorkspaceFolderRead(files::WorkspaceFolderRead),
    /// See [`files::WorkspaceFileRead`].
    WorkspaceFileRead(files::WorkspaceFileRead),
    /// See [`files::WorkspaceFileReadAt`].
    WorkspaceFileReadAt(files::WorkspaceFileReadAt),
    /// See [`files::WorkspaceFileOpenAt`].
    WorkspaceFileOpenAt(files::WorkspaceFileOpenAt),
    /// See [`files::WorkspaceFilePages`].
    WorkspaceFilePages(files::WorkspaceFilePages),
    /// Puts a prompt at the end of the queue. Sending it again under the same `client_msg_id` changes nothing, so a
    /// retry is safe, and `SessionChangeQueued` is what changes it. Refused with `Conflict` once that prompt has been
    /// sent, since the agent has it by then. Answers with the session. On an idle agent it's sent at once, unless the
    /// queue is held, as `Session::queue_held` says. As with `SessionPrompt`, each of `attachments` has to have been
    /// added to this session first, and a prompt with any can have no text.
    SessionQueue {
        id: SessionId,
        client_msg_id: ClientMessageId,
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<AttachmentId>,
    },
    /// Changes the text of the prompt queued under `client_msg_id`, which keeps its place and the files it was queued
    /// with, so with any the text can be empty. Refused with `Conflict` once it's been sent, since the agent has it by
    /// then, and with `NotFound` once it's been taken out, so a change that comes too late is never queued as a prompt
    /// of its own. Answers with the session. A hub older than this refuses it as unsupported.
    SessionChangeQueued {
        id: SessionId,
        client_msg_id: ClientMessageId,
        text: String,
    },
    /// Takes a prompt out of the queue. Taking out one that isn't there, because it's been sent or taken out already,
    /// changes nothing. Answers with the session.
    SessionUnqueue {
        id: SessionId,
        client_msg_id: ClientMessageId,
    },
    /// Sends the prompt queued under `client_msg_id` now, out of its turn: into the turn the agent is working on, which
    /// steers it, or as a turn of its own. Like any prompt the person sends, it lets a held queue go on once its turn
    /// ends. Sending one that isn't queued, because it's been sent or taken out, changes nothing. Answers with the
    /// session once the prompt is recorded.
    SessionSendQueued {
        id: SessionId,
        client_msg_id: ClientMessageId,
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
    /// Asks the agent to compact its conversation now, keeping what `keeping` says if it says anything. Refused with
    /// `Conflict` while the agent is on a turn, or has a prompt it hasn't taken, since it would only compact once
    /// that's done. Recorded as a `CompactionAsked`, so it's sent once.
    SessionCompact {
        id: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keeping: Option<String>,
    },
    /// Changes the model, approval mode or effort, on the running agent too if there is one. A setting left out is left
    /// as it is. Fails if the agent refuses, saving nothing, though a running agent may already have taken the
    /// settings it was given before the one it refused. It's refused, changing nothing, for a mode a new chat would be
    /// refused. With no agent running, a change of mode goes in the log as an `AgentMode`, as a running agent's report
    /// of one does. Sent again, it sets the same again.
    SessionConfigure {
        id: SessionId,
        #[serde(flatten)]
        options: SessionSettingsChange,
    },
    /// Stops its agent and hides it from the tabs. The history stays, and it can be reopened. Closing
    /// it again leaves it closed. A session nothing has been sent in, with nothing in its log and nothing queued, is
    /// deleted instead, with any files added to it, since there's nothing to reopen: the reply is the session as it
    /// was, the feed says it's removed, and closing it again is refused with `NotFound`.
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
    SessionDraft(drafts::SessionDraft),
    SessionAnswerDraft(drafts::SessionAnswerDraft),
    WorkspaceCommentDraft(drafts::WorkspaceCommentDraft),
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
