//! What a client asks a hub to do.
//!
//! Each request that changes something is safe to send again after a lost reply: it either has the same effect however
//! often it's applied, or carries an ID the client chose, which the hub records once. Each says which. Two act again
//! once what they acted on has passed, as each says: [`agent::SessionCancel`] and [`workspaces::WorkspaceRerunSetup`].
//! [`terminals::TerminalInput`] is never safe to send again, since typing twice isn't typing once.

use serde::{Deserialize, Serialize};

use super::{
    Hello, accounts, admin, agent, attachments, comments, drafts, files, log, new_workspace, queue, records, sessions,
    subscriptions, terminals, waits, workspaces,
};

/// Every request, each with a type of its own that names what it's answered with. One with no fields is a variant with
/// none, rather than one holding its type, since serde would then refuse it with a field a newer client added, and its
/// type only names what it's answered with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ClientRequest {
    Hello(Hello),
    /// See [`admin::HubStatus`].
    HubStatus,
    /// See [`admin::HubShutdown`].
    HubShutdown,
    /// See [`admin::HubShutdownIfIdle`].
    HubShutdownIfIdle,
    /// See [`admin::HubRestart`].
    HubRestart,
    HubConfigure(admin::HubConfigure),

    RepoAdd(records::RepoAdd),
    /// See [`records::RepoList`].
    RepoList,
    /// See [`records::RecordsList`].
    RecordsList,
    /// See [`admin::AgentList`].
    AgentList,

    WorkspaceCreate(workspaces::WorkspaceCreate),
    WorkspaceList(workspaces::WorkspaceList),
    WorkspaceArchive(workspaces::WorkspaceArchive),
    WorkspaceRestore(workspaces::WorkspaceRestore),
    WorkspaceRename(workspaces::WorkspaceRename),
    WorkspaceRegenerateTitle(workspaces::WorkspaceRegenerateTitle),
    WorkspaceStatus(workspaces::WorkspaceStatus),
    WorkspaceDiff(workspaces::WorkspaceDiff),
    WorkspaceSetupLog(workspaces::WorkspaceSetupLog),
    WorkspaceRerunSetup(workspaces::WorkspaceRerunSetup),
    WorkspacePullRequest(workspaces::WorkspacePullRequest),
    CommentAdd(comments::CommentAdd),
    CommentList(comments::CommentList),
    CommentEdit(comments::CommentEdit),
    CommentDelete(comments::CommentDelete),

    SessionCreate(sessions::SessionCreate),
    SessionEnsure(sessions::SessionEnsure),
    SessionList(sessions::SessionList),
    SessionGet(sessions::SessionGet),
    SessionRead(log::SessionRead),
    SessionPromptList(log::SessionPromptList),
    SessionCompactionList(log::SessionCompactionList),
    SessionFind(log::SessionFind),
    SessionSubscribe(subscriptions::SessionSubscribe),
    SessionSubagents(records::SessionSubagents),
    SessionReadPlaces(log::SessionReadPlaces),
    SessionSubscribePlaces(subscriptions::SessionSubscribePlaces),
    SessionUnsubscribe(subscriptions::SessionUnsubscribe),
    FeedSubscribe(subscriptions::FeedSubscribe),
    SessionPrompt(agent::SessionPrompt),
    AttachmentAdd(attachments::AttachmentAdd),
    AttachmentRead(attachments::AttachmentRead),
    AttachmentRemove(attachments::AttachmentRemove),
    SessionAccount(sessions::SessionAccount),
    AccountConnect(accounts::AccountConnect),
    AccountDisconnect(accounts::AccountDisconnect),
    WorkspaceFiles(files::WorkspaceFiles),
    WorkspaceFolderRead(files::WorkspaceFolderRead),
    WorkspaceFileRead(files::WorkspaceFileRead),
    WorkspaceFileReadAt(files::WorkspaceFileReadAt),
    WorkspaceFileOpenAt(files::WorkspaceFileOpenAt),
    WorkspaceFilePages(files::WorkspaceFilePages),
    SessionQueue(queue::SessionQueue),
    SessionChangeQueued(queue::SessionChangeQueued),
    SessionUnqueue(queue::SessionUnqueue),
    SessionSendQueued(queue::SessionSendQueued),
    SessionApprove(agent::SessionApprove),
    SessionCancel(agent::SessionCancel),
    SessionCompact(agent::SessionCompact),
    SessionConfigure(sessions::SessionConfigure),
    SessionClose(sessions::SessionClose),
    SessionReopen(sessions::SessionReopen),
    SessionMarkRead(sessions::SessionMarkRead),
    SessionMarkUnread(sessions::SessionMarkUnread),
    SessionCallOffWait(waits::SessionCallOffWait),
    SessionDraft(drafts::SessionDraft),
    SessionAnswerDraft(drafts::SessionAnswerDraft),
    WorkspaceCommentDraft(drafts::WorkspaceCommentDraft),
    NewWorkspaceDraftKeep(drafts::NewWorkspaceDraftKeep),
    NewWorkspaceDraftFileAdd(drafts::NewWorkspaceDraftFileAdd),
    NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead),
    NewWorkspaceDraftFileRemove(drafts::NewWorkspaceDraftFileRemove),
    NewWorkspaceCreate(new_workspace::NewWorkspaceCreate),
    SessionRename(sessions::SessionRename),
    TerminalOpen(terminals::TerminalOpen),
    TerminalAttach(terminals::TerminalAttach),
    TerminalInput(terminals::TerminalInput),
    TerminalResize(terminals::TerminalResize),
    TerminalClose(terminals::TerminalClose),
    /// Any request this build doesn't know, from a client newer than it. A hub answers with
    /// [`super::ErrorCode::Unsupported`] and carries on.
    #[serde(other)]
    Unknown,
}
