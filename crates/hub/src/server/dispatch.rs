use super::Connection;
use super::admit::Greeting;
use crate::attachment_files::FileFor;
use crate::hub::Hub;
use crate::session::ReadingOn;
use crate::store::AddedBy;
use protocol::client::{
    ApiError, ClientRequest, ClientResponse, ErrorCode, accounts, admin, agent, attachments, comments, conversation,
    drafts, files, github_accounts, github_apps, log, new_workspace, queue, records, references, sessions,
    tool_servers, tool_sign_ins, waits, workspaces,
};
use protocol::model::ByteRange;
use std::sync::Arc;
/// The reply to `request`.
#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
pub(super) async fn handle(
    hub: &Arc<Hub>,
    connection: &Arc<Connection>,
    greeting: Greeting,
    request: ClientRequest,
) -> Result<ClientResponse, ApiError> {
    use ClientRequest as R;
    use ClientResponse as C;
    let Greeting { device } = greeting;
    Ok(match request {
        R::Hello(_) => return Err(ApiError::new(ErrorCode::InvalidRequest, "already said hello")),
        R::Unknown => return Err(hub.unsupported(None)),
        R::TerminalOpen(open) => C::Terminal(hub.terminal_open(open).await?),
        R::TerminalRows(rows) => C::TerminalRows(hub.terminal_rows(rows).await?),
        R::TerminalBounds(asked) => C::TerminalBounds(hub.terminal_bounds(asked).await?),
        R::TerminalText(asked) => C::TerminalText(hub.terminal_text(asked).await?),
        // Sent on as the connection reads them, by `in_order`, so they reach the worker in the order they were
        // sent, and none comes here. One that did is still sent on and answered, out of order with the rest, rather than
        // left without a reply.
        R::TerminalTyped(typed) => hub.terminal_typed(typed).await?.await?,
        R::TerminalFit(fit) => hub.terminal_fit(fit).await?.await?,
        R::TerminalClose(close) => C::Terminal(hub.terminal_close(close).await?),
        // Taken in order as the connection reads it too, by `in_order`, and answered here only as typing is.
        R::Subscribe(asked) => C::Subscribed(super::subscribe::subscribe(hub, connection, asked).await),

        R::PairingStart => C::Pairing(hub.pairing_start().await?),
        R::PairingWait(protocol::client::devices::PairingWait { code }) => {
            let mut closed = connection.closed.subscribe();
            tokio::select! {
                result = hub.pairing_wait(code) => C::Device(result?),
                _ = closed.wait_for(|closed| *closed) => return Err(ApiError::new(ErrorCode::Unavailable, "connection closed")),
            }
        }
        R::DeviceList => C::Devices(protocol::client::devices::Devices { devices: hub.device_list().await? }),
        R::DeviceRevoke(protocol::client::devices::DeviceRevoke { id }) => {
            hub.device_revoke(id).await?;
            C::Ok
        }
        R::Pair(_) | R::Authenticate(_) => {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "already authenticated"));
        }
        R::HubStatus => C::HubStatus(hub.status()),
        R::HubShutdown => {
            hub.request_stop();
            C::Ok
        }
        R::HubShutdownIfIdle => {
            hub.shutdown_if_idle()?;
            C::Ok
        }
        R::HubRestart => {
            // Counted as it's read, so this request is among those under way until its reply has gone.
            hub.request_restart(1)?;
            C::Ok
        }
        R::HubUpdateTo(admin::HubUpdateTo { version }) => {
            hub.update_to(version).await?;
            C::Ok
        }
        R::HubConfigure(admin::HubConfigure { settings }) => C::HubSettings(hub.hub_configure(settings).await?),
        R::RepoAdd(records::RepoAdd { path }) => C::Repo(hub.repo_add(path).await?),
        R::RepoList => C::Repos(records::Repos { repos: hub.repo_list().await? }),
        R::ModelsFor(asked) => C::ModelList(hub.models_for(asked).await?),
        R::AgentList => C::Agents(admin::Agents { agents: hub.agent_list() }),
        R::WorkspaceCreate(workspaces::WorkspaceCreate { repo_id, name, base_ref, client_request_id }) => {
            C::Workspace(hub.workspace_create(repo_id, name, base_ref, client_request_id).await?)
        }
        R::WorkspaceList(workspaces::WorkspaceList { include_archived }) => {
            C::Workspaces(workspaces::Workspaces { workspaces: hub.workspace_list(include_archived).await? })
        }
        R::WorkspaceArchive(workspaces::WorkspaceArchive { id, force }) => {
            C::Workspace(hub.workspace_archive(id, force).await?)
        }
        R::WorkspaceRestore(workspaces::WorkspaceRestore { id }) => C::Workspace(hub.workspace_restore(id).await?),
        R::WorkspaceRename(workspaces::WorkspaceRename { id, title }) => {
            C::Workspace(hub.workspace_rename(id, &title).await?)
        }
        R::WorkspaceCommentDraft(drafts::WorkspaceCommentDraft { id, draft }) => {
            C::Workspace(hub.workspace_comment_draft(id, draft).await?)
        }
        R::WorkspaceRegenerateTitle(workspaces::WorkspaceRegenerateTitle { id }) => {
            C::Workspace(hub.workspace_regenerate_title(id).await?)
        }
        R::WorkspaceStatus(workspaces::WorkspaceStatus { id }) => C::GitStatus(hub.workspace_status(id).await?),
        R::WorkspaceDiff(workspaces::WorkspaceDiff { id }) => C::GitDiff(hub.workspace_diff(id).await?),
        R::WorkspaceFileDiff(asked) => C::FileDiff(hub.workspace_file_diff(asked).await?),
        R::WorkspaceFiles(files::WorkspaceFiles { id }) => C::WorktreeFiles(hub.workspace_files(id).await?),
        R::WorkspaceFolderRead(files::WorkspaceFolderRead { id, path }) => {
            C::WorktreeFiles(hub.workspace_folder_read(id, path).await?)
        }
        R::WorkspaceFileRead(files::WorkspaceFileRead { id, path }) => {
            C::FileContents(hub.workspace_file_read(id, path).await?)
        }
        R::WorkspaceFileReadAt(files::WorkspaceFileReadAt { id, commit, path }) => {
            C::FileContents(hub.workspace_file_read_at(id, commit, path).await?)
        }
        R::WorkspaceFileOpenAt(files::WorkspaceFileOpenAt { id, commit, path }) => {
            C::FileOpened(hub.workspace_file_open_at(id, commit, path).await?)
        }
        R::WorkspaceFilePages(asked) => C::FilePages(hub.workspace_file_pages(asked).await?),
        R::WorkspaceFileReadRange(files::WorkspaceFileReadRange { id, path, start, length }) => {
            C::FileRange(hub.workspace_file_read_range(id, path, ByteRange::new(start, length)?).await?)
        }
        R::WorkspaceSetupLog(workspaces::WorkspaceSetupLog { id }) => {
            C::SetupLog(workspaces::SetupLog { text: hub.workspace_setup_log(id).await? })
        }
        R::WorkspaceRerunSetup(workspaces::WorkspaceRerunSetup { id }) => {
            C::Workspace(hub.workspace_rerun_setup(id).await?)
        }
        R::WorkspacePullRequest(workspaces::WorkspacePullRequest { id }) => {
            C::PullRequest(workspaces::PullRequestReply { pull_request: hub.workspace_pull_request(id).await? })
        }
        R::ReferencesRead(references::ReferencesRead { workspace, references }) => {
            C::References(references::References { references: hub.references_read(&workspace, references).await? })
        }
        R::CommentAdd(comments::CommentAdd { workspace_id, path, side, line, text, client_request_id }) => {
            C::Comment(hub.comment_add(workspace_id, path, side, line, text, client_request_id).await?)
        }
        R::CommentList(comments::CommentList { workspace_id }) => {
            C::Comments(comments::Comments { comments: hub.comment_list(workspace_id).await? })
        }
        R::CommentEdit(comments::CommentEdit { id, text }) => C::Comment(hub.comment_edit(id, text).await?),
        R::CommentDelete(comments::CommentDelete { id }) => {
            hub.comment_delete(id).await?;
            C::Ok
        }
        R::SessionCreate(sessions::SessionCreate { workspace_id, agent: kind, settings, client_request_id }) => {
            C::Session(hub.session_create(workspace_id, kind, settings, client_request_id).await?)
        }
        R::SessionEnsure(sessions::SessionEnsure { workspace_id, agent: kind }) => {
            C::Session(hub.session_ensure(workspace_id, kind).await?)
        }
        R::SessionList(sessions::SessionList { workspace_id }) => {
            C::Sessions(sessions::Sessions { sessions: hub.session_list(workspace_id).await? })
        }
        R::SessionGet(sessions::SessionGet { id }) => C::Session(hub.session_get(id).await?),
        R::SessionRead(log::SessionRead { id, from_seq, limit, max_bytes, include_raw }) => {
            C::Events(hub.session_read(id, from_seq, limit, max_bytes, include_raw).await?)
        }
        R::SessionPromptList(log::SessionPromptList { id }) => {
            C::Prompts(log::Prompts { prompts: hub.session_prompt_list(id).await? })
        }
        R::SessionCompactionList(log::SessionCompactionList { id }) => {
            C::Compactions(log::Compactions { compactions: hub.session_compaction_list(id).await? })
        }
        R::SessionFind(log::SessionFind { id, subagent, query }) => {
            C::Found(hub.session_find(id, subagent, &query).await?)
        }
        R::SessionReadPlaces(log::SessionReadPlaces { id, subagent, from_place, limit, max_bytes, include_raw }) => {
            C::PlacedEvents(hub.session_read_places(id, subagent, from_place, limit, max_bytes, include_raw).await?)
        }
        R::ProposalAnswer(answer) => C::Proposal(hub.answer_proposal(answer).await?),
        R::ProposalAnotherRound(round) => C::Proposal(hub.ask_another_round(round).await?),
        R::ProposalRevisionsRead(read) => C::ProposalRevisions(hub.proposal_revisions(read).await?),
        R::SessionPrompt(agent::SessionPrompt { id, client_msg_id, prompt }) => {
            let (seq, duplicate) = hub.session_prompt(id, client_msg_id, prompt, device).await?;
            C::PromptRecorded(agent::PromptRecorded { seq, duplicate })
        }
        R::AttachmentAdd(attachments::AttachmentAdd { session_id, id, name, media_type, data }) => {
            let kept_for = FileFor::Session(session_id);
            C::Attachment(hub.file_add(kept_for, id, name, media_type, data, AddedBy::Client).await?)
        }
        R::AttachmentRead(attachments::AttachmentRead { session_id, id }) => {
            C::AttachmentData(attachments::AttachmentData {
                data: hub.file_read(FileFor::Session(session_id), id).await?,
            })
        }
        R::AttachmentReadRange(attachments::AttachmentReadRange { session_id, id, start, length }) => {
            let range = ByteRange::new(start, length)?;
            C::FileRange(hub.file_read_range(FileFor::Session(session_id), id, range).await?)
        }
        R::AttachmentRemove(attachments::AttachmentRemove { session_id, id }) => {
            hub.file_remove(FileFor::Session(session_id), id).await?;
            C::Ok
        }
        R::NewWorkspaceDraftKeep(drafts::NewWorkspaceDraftKeep { draft }) => {
            C::NewWorkspaceDraft(hub.new_workspace_draft_keep(draft).await?)
        }
        R::NewWorkspaceDraftFileAdd(drafts::NewWorkspaceDraftFileAdd { id, name, media_type, data }) => {
            C::Attachment(hub.file_add(FileFor::NewWorkspace, id, name, media_type, data, AddedBy::Client).await?)
        }
        R::NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead { id }) => {
            C::AttachmentData(attachments::AttachmentData { data: hub.file_read(FileFor::NewWorkspace, id).await? })
        }
        R::NewWorkspaceDraftFileRemove(drafts::NewWorkspaceDraftFileRemove { id }) => {
            hub.file_remove(FileFor::NewWorkspace, id).await?;
            C::Ok
        }
        R::NewWorkspaceCreate(new_workspace::NewWorkspaceCreate { made_of, client_request_id }) => {
            C::NewWorkspaceMade(Box::new(hub.new_workspace_create(made_of, client_request_id).await?))
        }
        R::AccountSetDefault(asked) => C::HubSettings(hub.account_set_default(asked).await?),
        R::AccountSetAutomatic(asked) => C::HubSettings(hub.account_set_automatic(asked).await?),
        R::SessionAssignAccount(asked) => C::Session(hub.session_assign_account(asked).await?),
        R::AccountConnect(asked) => C::Account(hub.account_connect(asked).await?),
        R::AccountDisconnect(accounts::AccountDisconnect { id }) => C::Account(hub.account_disconnect(id).await?),
        R::AccountSetReserve(asked) => C::Account(hub.account_set_reserve(asked).await?),
        R::GithubAccountConnect(asked) => C::GithubAccount(hub.github_account_connect(asked).await?),
        R::GithubAccountConnectGh => C::GithubAccount(hub.github_account_connect_gh().await?),
        R::HubGhLogin => C::GhLogin(hub.hub_gh_login().await?),
        R::HubMachineLogins => C::MachineLogins(hub.hub_machine_logins().await),
        R::GithubAccountDisconnect(github_accounts::GithubAccountDisconnect { id }) => {
            C::GithubAccount(hub.github_account_disconnect(id).await?)
        }
        R::GithubAccountSetDefault(asked) => C::HubSettings(hub.github_account_set_default(asked).await?),
        R::GithubSignInStart(asked) => C::GithubSignIn(hub.github_sign_in_start(asked).await?),
        R::GithubSignInCancel(github_accounts::GithubSignInCancel { id }) => {
            C::GithubSignIn(hub.github_sign_in_cancel(id).await?)
        }
        R::GithubAppManifestWrite(asked) => C::GithubAppManifest(github::manifest(&asked)?),
        R::GithubAppMake(github_apps::GithubAppMake { code }) => C::GithubApp(hub.github_app_make(code).await?),
        R::GithubAppInstallationsConnect(github_apps::GithubAppInstallationsConnect { id }) => {
            C::GithubApp(hub.github_app_installations_connect(id).await?)
        }
        R::GithubAppRemove(github_apps::GithubAppRemove { id }) => C::GithubApp(hub.github_app_remove(id).await?),
        R::ToolServerCheck(asked) => C::ToolServerChecked(hub.tool_server_check(asked).await?),
        R::ToolServerAdd(asked) => C::ToolServer(hub.tool_server_add(asked).await?),
        R::ToolServerEdit(asked) => C::ToolServer(hub.tool_server_edit(asked).await?),
        R::ToolServerRemove(tool_servers::ToolServerRemove { id }) => C::ToolServer(hub.tool_server_remove(id).await?),
        R::ToolSignInStart(asked) => C::ToolSignInBegun(hub.tool_sign_in_start(asked).await?),
        R::ToolSignInFinish(asked) => C::ToolServer(hub.tool_sign_in_finish(asked).await?),
        R::ToolSignInCancel(tool_sign_ins::ToolSignInCancel { id }) => {
            hub.tool_sign_in_cancel(&id);
            C::Ok
        }
        R::SessionQueue(queue::SessionQueue { id, client_msg_id, prompt }) => {
            C::Session(hub.session_queue(id, client_msg_id, prompt, device.as_ref()).await?)
        }
        R::SessionChangeQueued(queue::SessionChangeQueued { id, client_msg_id, text }) => {
            C::Session(hub.session_change_queued(id, client_msg_id, text).await?)
        }
        R::SessionUnqueue(queue::SessionUnqueue { id, client_msg_id }) => {
            C::Session(hub.session_unqueue(id, client_msg_id).await?)
        }
        R::SessionSendQueued(queue::SessionSendQueued { id, client_msg_id }) => {
            C::Session(hub.session_send_queued(id, client_msg_id).await?)
        }
        R::SessionDecide(agent::SessionDecide { id, approval_id, decision }) => {
            hub.session_decide(id, approval_id, decision).await?;
            C::Ok
        }
        R::SessionConfigure(sessions::SessionConfigure { id, settings }) => {
            C::Session(hub.session_configure(id, settings).await?)
        }
        R::SessionClose(sessions::SessionClose { id }) => C::Session(hub.session_close(id).await?),
        R::SessionReopen(sessions::SessionReopen { id }) => C::Session(hub.session_reopen(id).await?),
        R::SessionConversationStart(conversation::SessionConversationStart { id }) => {
            let device = device.ok_or_else(|| {
                ApiError::new(ErrorCode::InvalidRequest, "this client didn't say in its hello which device it's on")
            })?;
            C::Session(hub.session_conversation_start(id, device).await?)
        }
        R::SessionConversationEnd(conversation::SessionConversationEnd { id }) => {
            C::Session(hub.session_conversation_end(id).await?)
        }
        R::SessionReplyHeard(conversation::SessionReplyHeard { id, message, ending }) => {
            hub.session_reply_heard(id, device.as_ref(), message, ending).await?;
            C::Ok
        }
        R::SessionSpeaking(conversation::SessionSpeaking { id, through }) => {
            let on = ReadingOn { connection: connection.id, closed: connection.closed.subscribe() };
            hub.session_speaking(id, device.as_ref(), on, through).await?;
            C::Ok
        }
        R::SessionMarkRead(sessions::SessionMarkRead { id, through_seq }) => {
            C::Session(hub.session_mark_read(id, through_seq).await?)
        }
        R::SessionMarkUnread(sessions::SessionMarkUnread { id }) => C::Session(hub.session_mark_unread(id).await?),
        R::SessionCallOffWait(waits::SessionCallOffWait { id, wait }) => {
            C::Session(hub.session_call_off_wait(id, wait).await?)
        }
        R::SessionDraft(drafts::SessionDraft { id, draft, typed_at_ms }) => {
            C::Session(hub.session_draft(id, draft, typed_at_ms).await?)
        }
        R::SessionDecisionDraft(drafts::SessionDecisionDraft { id, draft }) => {
            C::Session(hub.session_decision_draft(id, draft).await?)
        }
        R::SessionRename(sessions::SessionRename { id, title }) => C::Session(hub.session_rename(id, &title).await?),
        R::SessionCancel(agent::SessionCancel { id }) => {
            hub.session_cancel(id).await?;
            C::Ok
        }
        R::SessionCompact(agent::SessionCompact { id, keeping }) => {
            hub.session_compact(id, keeping).await?;
            C::Ok
        }
        R::SessionReloadToolServer(agent::SessionReloadToolServer { id, server }) => {
            hub.session_reload_tool_server(id, server).await?;
            C::Ok
        }
    })
}
