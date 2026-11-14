//! Every request a hub sends its worker, and every reply a worker sends back, spelled out, since a worker can outlive
//! the hub that started it, and the next hub can be of another build.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType, NewAttachment};
use protocol::event::ApprovalDecision;
use protocol::frame;
use protocol::model::{
    AgentAccount, AgentKind, AgentLogin, Branches, Effort, FileContents, FileOpened, FilePage, FilePages, GitDiff,
    GitStatus, Head, LoginToken, ModelName, PageLine, PermissionMode, RunId, Seq, SessionSettings,
    SessionSettingsChange, WorktreeFiles,
};
use protocol::prompt::Prompt;
use protocol::worker::{
    self, AgentSpec, RateLimit, RunningAgent, ToolsToken, WorkerCall, WorkerFrame, WorkerOutcome, WorkerRequest,
    WorkerResponse,
};

use super::keeps_its_form;

/// A login a hub holds, as it hands it to a worker.
fn login() -> AgentLogin {
    AgentLogin { token: LoginToken::new("made-up") }
}

/// `request` is written as `{"id":7,"request":<form>}`, and that reads as `request`.
pub(super) fn asked(request: WorkerRequest, form: &str) {
    keeps_its_form(&WorkerCall { id: 7, request }, &format!(r#"{{"id":7,"request":{form}}}"#));
}

/// `response` is written as a reply to call 7 whose response is `form`, and that reads as `response`.
pub(super) fn answered(response: WorkerResponse, form: &str) {
    let reply = WorkerFrame::Reply { id: 7, outcome: WorkerOutcome::Response(response) };
    keeps_its_form(&reply, &format!(r#"{{"frame":"reply","id":7,"response":{form}}}"#));
}

#[test]
fn every_request_about_the_connection_and_the_workspace_keeps_its_form() {
    let hello = WorkerRequest::Hello(worker::Hello {
        protocol_version: 4,
        oldest_protocol_version: Some(3),
        keep_running_without_hub: true,
    });
    asked(
        hello,
        r#"{"method":"hello","protocol_version":4,"oldest_protocol_version":3,"keep_running_without_hub":true}"#,
    );
    asked(WorkerRequest::Stop, r#"{"method":"stop"}"#);
    asked(WorkerRequest::RunningAgents, r#"{"method":"running_agents"}"#);
    asked(WorkerRequest::Received(worker::Received { through: 12 }), r#"{"method":"received","through":12}"#);
    let names = WorkerRequest::NamesGitHas(worker::NamesGitHas { repo_name: "r".into(), repo_path: "/r".into() });
    asked(names, r#"{"method":"names_git_has","repo_name":"r","repo_path":"/r"}"#);
    let prepare = WorkerRequest::PrepareWorkspace(worker::PrepareWorkspace {
        repo_name: "r".into(),
        repo_path: "/r".into(),
        workspace_name: "aspen".into(),
        branch: "aspen".into(),
        base_ref: None,
    });
    asked(
        prepare,
        r#"{"method":"prepare_workspace","repo_name":"r","repo_path":"/r","workspace_name":"aspen","branch":"aspen","base_ref":null}"#,
    );
    let restore = WorkerRequest::RestoreWorkspace(worker::RestoreWorkspace {
        repo_path: "/r".into(),
        path: "/w".into(),
        branch: "birch".into(),
    });
    asked(restore, r#"{"method":"restore_workspace","repo_path":"/r","path":"/w","branch":"birch"}"#);
    let remove = worker::RemoveWorkspace { repo_path: "/r".into(), path: "/w".into(), force: true, own: None };
    let another = worker::RemoveWorkspace { path: "/c".into(), own: Some("/w".into()), ..remove.clone() };
    asked(
        WorkerRequest::RemoveWorkspace(remove),
        r#"{"method":"remove_workspace","repo_path":"/r","path":"/w","force":true}"#,
    );
    asked(
        WorkerRequest::RemoveWorkspace(another),
        r#"{"method":"remove_workspace","repo_path":"/r","path":"/c","force":true,"own":"/w"}"#,
    );
    let env = vec![("PORT".to_string(), "55000".to_string())];
    let setup =
        WorkerRequest::SetupWorkspace(worker::SetupWorkspace { repo_path: "/r".into(), path: "/w".into(), env });
    asked(setup, r#"{"method":"setup_workspace","repo_path":"/r","path":"/w","env":[["PORT","55000"]]}"#);
    asked(WorkerRequest::CancelSetup, r#"{"method":"cancel_setup"}"#);
    asked(WorkerRequest::SetupLog(worker::SetupLog { path: "/w".into() }), r#"{"method":"setup_log","path":"/w"}"#);
    let archive =
        WorkerRequest::ArchiveScript(worker::ArchiveScript { repo_path: "/r".into(), path: "/w".into(), env: vec![] });
    asked(archive, r#"{"method":"archive_script","repo_path":"/r","path":"/w","env":[]}"#);
    asked(
        WorkerRequest::ServeTools(worker::ServeTools { port: Some(50_123) }),
        r#"{"method":"serve_tools","port":50123}"#,
    );
    asked(WorkerRequest::ServeTools(worker::ServeTools { port: None }), r#"{"method":"serve_tools"}"#);
}

#[test]
fn every_request_about_the_worktree_keeps_its_form() {
    asked(WorkerRequest::GitStatus(worker::GitStatus { path: "/w".into() }), r#"{"method":"git_status","path":"/w"}"#);
    let diff = WorkerRequest::GitDiff(worker::GitDiff { path: "/w".into(), base_ref: Some("origin/main".into()) });
    asked(diff, r#"{"method":"git_diff","path":"/w","base_ref":"origin/main"}"#);
    asked(
        WorkerRequest::PullRequest(worker::PullRequest { path: "/w".into() }),
        r#"{"method":"pull_request","path":"/w"}"#,
    );
    asked(
        WorkerRequest::WatchBranches(worker::WatchBranches { path: "/w".into() }),
        r#"{"method":"watch_branches","path":"/w"}"#,
    );
    asked(
        WorkerRequest::GithubRateLimit(worker::GithubRateLimit { path: "/w".into() }),
        r#"{"method":"github_rate_limit","path":"/w"}"#,
    );
    let data = AttachmentBytes::new(b"ok".to_vec());
    let write =
        WorkerRequest::AttachmentWrite(worker::AttachmentWrite { path: "/w".into(), name: "build.log".into(), data });
    asked(write, r#"{"method":"attachment_write","path":"/w","name":"build.log","data":"b2s="}"#);
    asked(WorkerRequest::ListFiles(worker::ListFiles { path: "/w".into() }), r#"{"method":"list_files","path":"/w"}"#);
    let folder = WorkerRequest::FolderRead(worker::FolderRead { path: "/w".into(), folder: "target".into() });
    asked(folder, r#"{"method":"folder_read","path":"/w","folder":"target"}"#);
    let file = WorkerRequest::FileRead(worker::FileRead { path: "/w".into(), file: "a.rs".into() });
    asked(file, r#"{"method":"file_read","path":"/w","file":"a.rs"}"#);
    let at =
        WorkerRequest::FileReadAt(worker::FileReadAt { path: "/w".into(), commit: "c1".into(), file: "a.rs".into() });
    asked(at, r#"{"method":"file_read_at","path":"/w","commit":"c1","file":"a.rs"}"#);
    let open =
        WorkerRequest::FileOpenAt(worker::FileOpenAt { path: "/w".into(), commit: "c1".into(), file: "a.rs".into() });
    asked(open, r#"{"method":"file_open_at","path":"/w","commit":"c1","file":"a.rs"}"#);
    let pages = WorkerRequest::FilePages(worker::FilePages {
        path: "/w".into(),
        commit: "c1".into(),
        file: "a.rs".into(),
        page_lines: 500,
        pages: vec![0, 2],
    });
    asked(pages, r#"{"method":"file_pages","path":"/w","commit":"c1","file":"a.rs","page_lines":500,"pages":[0,2]}"#);
}

#[test]
fn every_request_about_an_agent_keeps_its_form() {
    let spec = AgentSpec {
        session_id: "s".into(),
        run: RunId::new(4),
        agent: AgentKind::Claude,
        cwd: "/w".into(),
        start: protocol::capability::conversation::Start::Resume { conversation: "a".into() },
        settings: SessionSettings {
            model: ModelName::named("opus"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::High,
        },
        env: vec![],
        tools_token: Some(ToolsToken::new("t")),
        login: None,
    };
    let under_a_login = AgentSpec { tools_token: None, login: Some(login()), ..spec.clone() };
    asked(
        WorkerRequest::AgentStart(under_a_login),
        r#"{"method":"agent_start","session_id":"s","run":4,"agent":"claude","cwd":"/w","agent_session_id":"a","resume":true,"model":"opus","permission_mode":"bypassPermissions","effort":"high","env":[],"login":{"token":"made-up"}}"#,
    );
    asked(
        WorkerRequest::AgentStart(spec),
        r#"{"method":"agent_start","session_id":"s","run":4,"agent":"claude","cwd":"/w","agent_session_id":"a","resume":true,"model":"opus","permission_mode":"bypassPermissions","effort":"high","env":[],"tools_token":"t"}"#,
    );
    let prompt = WorkerRequest::AgentPrompt(worker::AgentPrompt {
        session_id: "s".into(),
        seq: Seq::new(3),
        prompt: Prompt::new("go"),
    });
    asked(prompt, r#"{"method":"agent_prompt","session_id":"s","prompt":3,"text":"go"}"#);
    let decision = ApprovalDecision::Deny { message: None, interrupt: true };
    let approval = WorkerRequest::AgentApproval(worker::AgentApproval {
        session_id: "s".into(),
        approval_id: "a".into(),
        decision,
    });
    asked(
        approval,
        r#"{"method":"agent_approval","session_id":"s","approval_id":"a","decision":{"decision":"deny","interrupt":true}}"#,
    );
    asked(
        WorkerRequest::AgentCancel(worker::AgentCancel { session_id: "s".into() }),
        r#"{"method":"agent_cancel","session_id":"s"}"#,
    );
    let compact = WorkerRequest::AgentCompact(worker::AgentCompact {
        session_id: "s".into(),
        asked: Seq::new(9),
        keeping: Some("The API".into()),
    });
    asked(compact, r#"{"method":"agent_compact","session_id":"s","asked":9,"keeping":"The API"}"#);
    let options = SessionSettingsChange { model: Some(ModelName::named("opus")), permission_mode: None, effort: None };
    asked(
        WorkerRequest::AgentConfigure(worker::AgentConfigure { session_id: "s".into(), options }),
        r#"{"method":"agent_configure","session_id":"s","model":"opus","permission_mode":null,"effort":null}"#,
    );
    asked(
        WorkerRequest::AgentStop(worker::AgentStop { session_id: "s".into() }),
        r#"{"method":"agent_stop","session_id":"s"}"#,
    );
}

#[test]
fn every_request_of_an_agents_cli_outside_any_session_keeps_its_form() {
    let shot = Attachment { id: "a1".into(), name: "shot.png".into(), media_type: MediaType::Png, size_bytes: 4 };
    let images = vec![NewAttachment { attachment: shot, data: AttachmentBytes::new(b"\x89PNG".to_vec()) }];
    let title = worker::TitleFor { agent: AgentKind::Claude, prompt: "Fix this".into(), images, login: None };
    let under_a_login = worker::TitleFor { images: vec![], login: Some(login()), ..title.clone() };
    asked(
        WorkerRequest::TitleFor(under_a_login),
        r#"{"method":"title_for","agent":"claude","prompt":"Fix this","login":{"token":"made-up"}}"#,
    );
    let title = WorkerRequest::TitleFor(title);
    asked(
        title,
        r#"{"method":"title_for","agent":"claude","prompt":"Fix this","images":[{"attachment":{"id":"a1","name":"shot.png","media_type":"image/png","size_bytes":4},"data":"iVBORw=="}]}"#,
    );
    asked(
        WorkerRequest::AccountFor(worker::AccountFor { agent: AgentKind::Claude, login: None }),
        r#"{"method":"account_for","agent":"claude"}"#,
    );
    asked(
        WorkerRequest::AccountFor(worker::AccountFor { agent: AgentKind::Claude, login: Some(login()) }),
        r#"{"method":"account_for","agent":"claude","login":{"token":"made-up"}}"#,
    );
}

/// A unit request reads with fields a newer hub added to it, as any request does.
#[test]
fn a_request_with_no_fields_reads_with_fields_it_doesnt_know() {
    let read = |form: &str| frame::decode::<WorkerRequest>(form.as_bytes()).unwrap();

    assert_eq!(read(r#"{"method":"stop","newer":1}"#), WorkerRequest::Stop);
    assert_eq!(read(r#"{"method":"running_agents","newer":1}"#), WorkerRequest::RunningAgents);
    assert_eq!(read(r#"{"method":"cancel_setup","newer":1}"#), WorkerRequest::CancelSetup);
}

#[test]
fn every_reply_about_the_connection_and_the_workspace_keeps_its_form() {
    let hello = WorkerResponse::Hello(worker::HelloReply { protocol_version: 3, build: Some("0.1.9".into()) });
    answered(hello, r#"{"result":"hello","protocol_version":3,"build":"0.1.9"}"#);
    let running = vec![RunningAgent { session_id: "s".into(), run: RunId::new(4) }];
    answered(
        WorkerResponse::RunningAgents(worker::RunningAgentsReply { agents: running }),
        r#"{"result":"running_agents","agents":[{"session_id":"s","run":4}]}"#,
    );
    answered(WorkerResponse::Ok, r#"{"result":"ok"}"#);
    let names = WorkerResponse::NamesGitHas(worker::NamesGitHasReply { names: vec!["aspen".into(), "main".into()] });
    answered(names, r#"{"result":"names_git_has","names":["aspen","main"]}"#);
    let ready = WorkerResponse::WorkspaceReady(worker::WorkspaceReady {
        path: "/w".into(),
        branch: "aspen".into(),
        base_ref: "origin/main".into(),
        warnings: vec!["no .env to copy".into()],
        branches: Some(Branches { head: Head::Branch { name: "aspen".into() }, had: vec![] }),
    });
    answered(
        ready,
        r#"{"result":"workspace_ready","path":"/w","branch":"aspen","base_ref":"origin/main","warnings":["no .env to copy"],"branches":{"head":{"kind":"branch","name":"aspen"},"had":[]}}"#,
    );
    answered(WorkerResponse::SetupDone(worker::SetupDone { code: Some(0) }), r#"{"result":"setup_done","code":0}"#);
    answered(WorkerResponse::SetupDone(worker::SetupDone { code: None }), r#"{"result":"setup_done","code":null}"#);
    let log = WorkerResponse::SetupLog(worker::SetupLogReply { text: "Running the setup script\n".into() });
    answered(log, r#"{"result":"setup_log","text":"Running the setup script\n"}"#);
    answered(
        WorkerResponse::ToolsServing(worker::ToolsServing { port: 50_123 }),
        r#"{"result":"tools_serving","port":50123}"#,
    );
}

#[test]
fn every_reply_about_the_worktree_keeps_its_form() {
    let status = GitStatus { branch: Some("birch".into()), head: Some("c1".into()), ..GitStatus::default() };
    answered(
        WorkerResponse::GitStatus(status),
        r#"{"result":"git_status","branch":"birch","head":"c1","ahead":0,"behind":0,"files":[],"unreferenced_commits":false}"#,
    );
    let no_commit_yet = GitStatus { branch: Some("birch".into()), ..GitStatus::default() };
    answered(
        WorkerResponse::GitStatus(no_commit_yet),
        r#"{"result":"git_status","branch":"birch","head":null,"ahead":0,"behind":0,"files":[],"unreferenced_commits":false}"#,
    );
    let diff = GitDiff { base_commit: Some("c1".into()), patch: String::new(), truncated: false };
    answered(WorkerResponse::GitDiff(diff), r#"{"result":"git_diff","base_commit":"c1","patch":"","truncated":false}"#);
    let found =
        WorkerResponse::PullRequest(worker::PullRequestReply { pull_request: None, branch: Some("aspen".into()) });
    answered(found, r#"{"result":"pull_request","pull_request":null,"branch":"aspen"}"#);
    answered(
        WorkerResponse::PullRequest(worker::PullRequestReply { pull_request: None, branch: None }),
        r#"{"result":"pull_request","pull_request":null}"#,
    );
    let limit = RateLimit { limit: 5000, remaining: 4990, resets_at_ms: 9 };
    answered(
        WorkerResponse::GithubRateLimit(limit),
        r#"{"result":"github_rate_limit","limit":5000,"remaining":4990,"resets_at_ms":9}"#,
    );
    let written = WorkerResponse::AttachmentWritten(worker::AttachmentWritten {
        path: "/w/.context/attachments/build.log".into(),
    });
    answered(written, r#"{"result":"attachment_written","path":"/w/.context/attachments/build.log"}"#);
    let files = WorktreeFiles { paths: vec!["a.rs".into()], folders: vec!["target".into()], truncated: false };
    answered(
        WorkerResponse::Files(files),
        r#"{"result":"files","paths":["a.rs"],"folders":["target"],"truncated":false}"#,
    );
    let text = FileContents::Text { text: "one".into() };
    answered(WorkerResponse::FileContents(text), r#"{"result":"file_contents","kind":"text","text":"one"}"#);
    answered(WorkerResponse::FileOpened(FileOpened::NotText), r#"{"result":"file_opened","kind":"not_text"}"#);
    let line = PageLine { text: "fn main() {}".into(), cut: None };
    let pages = FilePages { pages: vec![FilePage { index: 0, lines: vec![line] }] };
    answered(
        WorkerResponse::FilePages(pages),
        r#"{"result":"file_pages","pages":[{"index":0,"lines":[{"text":"fn main() {}"}]}]}"#,
    );
}

#[test]
fn every_reply_about_an_agent_keeps_its_form() {
    let started =
        WorkerResponse::AgentStarted(worker::AgentStarted { pid: 4242, run: RunId::new(4), already_running: false });
    answered(started, r#"{"result":"agent_started","pid":4242,"run":4,"already_running":false}"#);
    answered(WorkerResponse::AgentNotRunning, r#"{"result":"agent_not_running"}"#);
    answered(
        WorkerResponse::Title(worker::TitleReply { title: "Fix the login bug".into() }),
        r#"{"result":"title","title":"Fix the login bug"}"#,
    );
    let account = AgentAccount {
        id: Some("acc".into()),
        logged_in: true,
        model_provider: Some("Anthropic API".into()),
        plan: Some("Max".into()),
        login: Some("Claude account".into()),
        email: Some("person@example.com".into()),
    };
    answered(
        WorkerResponse::Account(account),
        r#"{"result":"account","id":"acc","logged_in":true,"model_provider":"Anthropic API","plan":"Max","login":"Claude account","email":"person@example.com"}"#,
    );
}
