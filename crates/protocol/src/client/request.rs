//! What a client asks a hub to do.
//!
//! Each request that changes something is safe to send again after a lost reply: it either has the same effect however
//! often it's applied, or carries an ID the client chose, which the hub records once. Each says which. Two act again
//! once what they acted on has passed, as each says: [`agent::SessionCancel`] and [`workspaces::WorkspaceRerunSetup`].
//! [`terminals::TerminalTyped`] is never safe to send again, since typing twice isn't typing once.

use serde::{Deserialize, Serialize};

use super::{
    Hello, accounts, admin, agent, attachments, comments, conversation, drafts, files, github_accounts, github_apps,
    log, new_workspace, proposals, queue, records, references, sessions, subscriptions, terminals, tool_server_checks,
    tool_servers, tool_sign_ins, waits, workspaces,
};

/// Every request, each with a type of its own that names what it's answered with. One with no fields is a variant with
/// none, rather than one holding its type, since serde would then refuse it with a field a newer client added, and its
/// type only names what it's answered with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ClientRequest {
    Pair(super::devices::Pair),
    Authenticate(super::devices::Authenticate),
    PairingStart,
    PairingWait(super::devices::PairingWait),
    DeviceList,
    DeviceRevoke(super::devices::DeviceRevoke),
    Hello(Hello),
    /// See [`admin::HubStatus`].
    HubStatus,
    /// See [`admin::HubShutdown`].
    HubShutdown,
    /// See [`admin::HubShutdownIfIdle`].
    HubShutdownIfIdle,
    /// See [`admin::HubRestart`].
    HubRestart,
    HubUpdateTo(admin::HubUpdateTo),
    HubConfigure(admin::HubConfigure),

    RepoAdd(records::RepoAdd),
    /// See [`records::RepoList`].
    RepoList,
    /// See [`admin::AgentList`].
    AgentList,
    /// See [`admin::ModelsFor`].
    ModelsFor(admin::ModelsFor),

    WorkspaceCreate(workspaces::WorkspaceCreate),
    WorkspaceList(workspaces::WorkspaceList),
    WorkspaceArchive(workspaces::WorkspaceArchive),
    WorkspaceRestore(workspaces::WorkspaceRestore),
    WorkspaceRename(workspaces::WorkspaceRename),
    WorkspaceRegenerateTitle(workspaces::WorkspaceRegenerateTitle),
    WorkspaceStatus(workspaces::WorkspaceStatus),
    WorkspaceDiff(workspaces::WorkspaceDiff),
    WorkspaceFileDiff(workspaces::WorkspaceFileDiff),
    WorkspaceSetupLog(workspaces::WorkspaceSetupLog),
    WorkspaceRerunSetup(workspaces::WorkspaceRerunSetup),
    WorkspacePullRequest(workspaces::WorkspacePullRequest),
    ReferencesRead(references::ReferencesRead),
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
    ProposalAnswer(proposals::ProposalAnswer),
    ProposalAnotherRound(proposals::ProposalAnotherRound),
    ProposalRevisionsRead(proposals::ProposalRevisionsRead),
    SessionReadPlaces(log::SessionReadPlaces),
    /// See [`subscriptions::Subscribe`].
    Subscribe(subscriptions::Subscribe),
    SessionPrompt(agent::SessionPrompt),
    AttachmentAdd(attachments::AttachmentAdd),
    AttachmentRead(attachments::AttachmentRead),
    AttachmentReadRange(attachments::AttachmentReadRange),
    AttachmentRemove(attachments::AttachmentRemove),
    SessionAssignAccount(sessions::SessionAssignAccount),
    AccountConnect(accounts::AccountConnect),
    AccountSetDefault(accounts::AccountSetDefault),
    AccountSetAutomatic(accounts::AccountSetAutomatic),
    AccountDisconnect(accounts::AccountDisconnect),
    AccountSetReserve(accounts::AccountSetReserve),
    GithubAccountConnect(github_accounts::GithubAccountConnect),
    GithubAccountDisconnect(github_accounts::GithubAccountDisconnect),
    GithubAccountSetDefault(github_accounts::GithubAccountSetDefault),
    GithubSignInStart(github_accounts::GithubSignInStart),
    GithubSignInCancel(github_accounts::GithubSignInCancel),
    HubGhLogin,
    HubMachineLogins,
    GithubAccountConnectGh,
    GithubAppManifestWrite(github_apps::GithubAppManifestWrite),
    GithubAppMake(github_apps::GithubAppMake),
    GithubAppInstallationsConnect(github_apps::GithubAppInstallationsConnect),
    GithubAppRemove(github_apps::GithubAppRemove),
    ToolServerCheck(tool_server_checks::ToolServerCheck),
    ToolServerAdd(tool_servers::ToolServerAdd),
    ToolServerEdit(tool_servers::ToolServerEdit),
    ToolServerRemove(tool_servers::ToolServerRemove),
    ToolSignInStart(tool_sign_ins::ToolSignInStart),
    ToolSignInFinish(tool_sign_ins::ToolSignInFinish),
    ToolSignInCancel(tool_sign_ins::ToolSignInCancel),
    WorkspaceFiles(files::WorkspaceFiles),
    WorkspaceFolderRead(files::WorkspaceFolderRead),
    WorkspaceFileRead(files::WorkspaceFileRead),
    WorkspaceFileReadAt(files::WorkspaceFileReadAt),
    WorkspaceFileOpenAt(files::WorkspaceFileOpenAt),
    WorkspaceFilePages(files::WorkspaceFilePages),
    WorkspaceFileReadRange(files::WorkspaceFileReadRange),
    SessionQueue(queue::SessionQueue),
    SessionChangeQueued(queue::SessionChangeQueued),
    SessionUnqueue(queue::SessionUnqueue),
    SessionSendQueued(queue::SessionSendQueued),
    SessionDecide(agent::SessionDecide),
    SessionCancel(agent::SessionCancel),
    SessionCompact(agent::SessionCompact),
    SessionConfigure(sessions::SessionConfigure),
    SessionClose(sessions::SessionClose),
    SessionReopen(sessions::SessionReopen),
    SessionMarkRead(sessions::SessionMarkRead),
    SessionMarkUnread(sessions::SessionMarkUnread),
    SessionConversationStart(conversation::SessionConversationStart),
    SessionConversationEnd(conversation::SessionConversationEnd),
    SessionReplyHeard(conversation::SessionReplyHeard),
    SessionSpeaking(conversation::SessionSpeaking),
    SessionCallOffWait(waits::SessionCallOffWait),
    SessionDraft(drafts::SessionDraft),
    SessionDecisionDraft(drafts::SessionDecisionDraft),
    WorkspaceCommentDraft(drafts::WorkspaceCommentDraft),
    NewWorkspaceDraftKeep(drafts::NewWorkspaceDraftKeep),
    NewWorkspaceDraftFileAdd(drafts::NewWorkspaceDraftFileAdd),
    NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead),
    NewWorkspaceDraftFileRemove(drafts::NewWorkspaceDraftFileRemove),
    NewWorkspaceCreate(new_workspace::NewWorkspaceCreate),
    SessionRename(sessions::SessionRename),
    TerminalOpen(terminals::TerminalOpen),
    TerminalRows(terminals::TerminalRows),
    TerminalBounds(terminals::TerminalBounds),
    TerminalText(terminals::TerminalText),
    TerminalTyped(terminals::TerminalTyped),
    TerminalFit(terminals::TerminalFit),
    TerminalClose(terminals::TerminalClose),
    /// Any request this build doesn't know, from a client newer than it. A hub answers with
    /// [`super::ErrorCode::Unsupported`] and carries on.
    #[serde(other)]
    Unknown,
}
