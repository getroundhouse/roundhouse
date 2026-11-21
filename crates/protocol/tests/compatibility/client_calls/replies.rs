//! Every reply, as a hub writes it and a client reads it, each with at least its `result`. Most that hold a model type
//! are spelled out whole. The rest, such as a session's record or a git status, are that type's form beside their
//! `result`, so only their `result` is pinned here, and the type's form is left to its own pins, where it has any.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::new_workspace::UnsentPrompt;
use protocol::client::{
    ApiError, ClientResponse, ErrorCode, HelloReply, HubStart, HubStatus, admin, agent, attachments, comments, log,
    new_workspace, records, sessions, workspaces,
};
use protocol::event::{CompactedBy, SessionEvent};
use protocol::find::Found;
use protocol::fixtures;
use protocol::model::{
    Account, CompactionLine, DiffComment, DiffSide, FileContents, FileOpened, FilePages, GhLogin, GhLoginWorks,
    GitDiff, GitStatus, GithubAccount, GithubLoginKind, GithubSettings, GithubSignIn, GithubUser, GithubUserId,
    HeldGithubLogin, HubSettings, MachineLogins, NewWorkspaceDraft, Place, PromptLine, Records, Repo, Reserve, Rev,
    Seq, SessionAccount, SshLogin, WorktreeFiles,
};

use super::answered;

/// One of the agent's own events, which is where it sits in a reply's `events`.
const EVENT: &str =
    r#"{"session_id":"s","seq":4,"ts_ms":1,"place":3,"body":{"type":"tool_call_dropped","tool_use_id":"toolu_1"}}"#;

fn event() -> SessionEvent {
    serde_json::from_str(EVENT).unwrap()
}

#[test]
fn a_hubs_settings_keep_their_form_and_one_from_a_hub_from_before_says_nothing_of_allowing_tools_or_github() {
    let settings = |github| HubSettings {
        account_defaults: None,
        keep_awake: false,
        allow_asked_in_full_access: Some(true),
        github,
        rev: Rev::new(3),
    };
    answered(
        ClientResponse::HubSettings(settings(None)),
        r#"{"result":"hub_settings","keep_awake":false,"allow_asked_in_full_access":true,"rev":3}"#,
    );
    answered(
        ClientResponse::HubSettings(settings(Some(GithubSettings {
            account: Some("g".into()),
            sent: None,
            left_to_machine: Vec::new(),
            set_up: Vec::new(),
        }))),
        r#"{"result":"hub_settings","keep_awake":false,"allow_asked_in_full_access":true,"github":{"account":"g"},"rev":3}"#,
    );
    // No default, on a hub that keeps GitHub accounts.
    answered(
        ClientResponse::HubSettings(settings(Some(GithubSettings {
            account: None,
            sent: None,
            left_to_machine: Vec::new(),
            set_up: Vec::new(),
        }))),
        r#"{"result":"hub_settings","keep_awake":false,"allow_asked_in_full_access":true,"github":{"account":null},"rev":3}"#,
    );
    // From a hub from before it could allow a tool's use itself, which says nothing of it, nor of GitHub.
    let older: ClientResponse = serde_json::from_str(r#"{"result":"hub_settings","keep_awake":true,"rev":3}"#).unwrap();
    let settings = HubSettings {
        account_defaults: None,
        keep_awake: true,
        allow_asked_in_full_access: None,
        github: None,
        rev: Rev::new(3),
    };
    assert_eq!(older, ClientResponse::HubSettings(settings));
}

#[test]
fn a_github_accounts_record_keeps_its_form_held_or_disconnected() {
    let held =
        HeldGithubLogin { how: GithubLoginKind::Token, connected_at_ms: 1, ends_at_ms: Some(9), needs_sign_in: false };
    let account = |held_login| GithubAccount {
        id: "g".into(),
        user: GithubUser { id: GithubUserId::new(7), name: "ada".into() },
        held_login,
        installation: None,
        rev: Rev::new(2),
    };
    answered(
        ClientResponse::GithubAccount(account(Some(held))),
        r#"{"result":"github_account","id":"g","user":{"id":7,"name":"ada"},"held_login":{"how":"token","connected_at_ms":1,"ends_at_ms":9,"needs_sign_in":false},"rev":2}"#,
    );
    answered(
        ClientResponse::GithubAccount(account(None)),
        r#"{"result":"github_account","id":"g","user":{"id":7,"name":"ada"},"held_login":null,"rev":2}"#,
    );
    let gh = HeldGithubLogin { how: GithubLoginKind::Gh, ends_at_ms: None, ..held };
    answered(
        ClientResponse::GithubAccount(account(Some(gh))),
        r#"{"result":"github_account","id":"g","user":{"id":7,"name":"ada"},"held_login":{"how":"gh","connected_at_ms":1,"ends_at_ms":null,"needs_sign_in":false},"rev":2}"#,
    );
}

#[test]
fn who_gh_on_the_hubs_machine_is_keeps_its_form() {
    answered(
        ClientResponse::GhLogin(GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes }),
        r#"{"result":"gh_login","state":"logged_in","username":"ada","works":"yes"}"#,
    );
    answered(ClientResponse::GhLogin(GhLogin::LoggedOut), r#"{"result":"gh_login","state":"logged_out"}"#);
}

/// Who `gh` and the SSH key on the hub's machine are, with `gh` left out when the hub couldn't tell.
#[test]
fn who_gh_and_the_ssh_key_on_the_hubs_machine_are_keeps_its_form() {
    let gh = GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes };
    let ssh = SshLogin::Key { username: "ada".into() };
    answered(
        ClientResponse::MachineLogins(MachineLogins { gh: Some(gh), ssh }),
        r#"{"result":"machine_logins","gh":{"state":"logged_in","username":"ada","works":"yes"},"ssh":{"state":"key","username":"ada"}}"#,
    );
    answered(
        ClientResponse::MachineLogins(MachineLogins { gh: None, ssh: SshLogin::CouldntTell }),
        r#"{"result":"machine_logins","ssh":{"state":"couldnt_tell"}}"#,
    );
}

#[test]
fn a_sign_ins_record_keeps_its_form_as_a_reply() {
    let sign_in = GithubSignIn { rev: Rev::new(3), ..fixtures::github_sign_in("s") };

    answered(
        ClientResponse::GithubSignIn(sign_in),
        r#"{"result":"github_sign_in","id":"s","status":{"state":"waiting","code":"WDJB-MJHT","address":"https://github.com/login/device","code_ends_at_ms":900000},"rev":3}"#,
    );
}

#[test]
fn every_reply_of_the_hub_itself_or_listing_its_records_keeps_its_form() {
    answered(ClientResponse::Ok, r#"{"result":"ok"}"#);
    answered(
        ClientResponse::Hello(HelloReply { protocol_version: 3, hub_version: "0.1.9".into() }),
        r#"{"result":"hello","protocol_version":3,"hub_version":"0.1.9"}"#,
    );
    answered(ClientResponse::Agents(admin::Agents { agents: vec![] }), r#"{"result":"agents","agents":[]}"#);
    let account = Account {
        id: "claude-1".into(),
        model_provider: None,
        plan: None,
        login: None,
        email: None,
        name: Some("Work".into()),
        login_agent: None,
        limits: vec![],
        held_login: None,
        used_at_ms: Some(5),
        reserve: Reserve::new(30),
        rev: Rev::new(2),
    };
    answered(
        ClientResponse::Account(account),
        r#"{"result":"account","id":"claude-1","model_provider":null,"plan":null,"login":null,"email":null,"name":"Work","login_agent":null,"limits":[],"held_login":null,"used_at_ms":5,"reserve":30,"rev":2}"#,
    );
    let repo = Repo { created_at_ms: 1, rev: Rev::new(2), ..fixtures::repo("r") };
    answered(
        ClientResponse::Repo(repo.clone()),
        r#"{"result":"repo","id":"r","name":"r","path":"/r","origin":{"kind":"checkout","path":"/r"},"created_at_ms":1,"rev":2}"#,
    );
    answered(
        ClientResponse::Repos(records::Repos { repos: vec![repo] }),
        r#"{"result":"repos","repos":[{"id":"r","name":"r","path":"/r","origin":{"kind":"checkout","path":"/r"},"created_at_ms":1,"rev":2}]}"#,
    );
    answered(
        ClientResponse::Sessions(sessions::Sessions { sessions: vec![] }),
        r#"{"result":"sessions","sessions":[]}"#,
    );
}

/// A chat's subagents and proposals, which are listed by chat, apart from every other record.
#[test]
fn every_list_of_a_chats_own_records_keeps_its_form() {
    answered(
        ClientResponse::Subagents(records::Subagents { subagents: vec![], rev: Rev::new(9) }),
        r#"{"result":"subagents","subagents":[],"rev":9}"#,
    );
    answered(
        ClientResponse::Proposals(records::Proposals { proposals: vec![], rev: Rev::new(9) }),
        r#"{"result":"proposals","proposals":[],"rev":9}"#,
    );
}

/// What a client reads of a hub across a break, to see it and stop or restart it, as `docs/principles/protocol.md`
/// says.
#[test]
fn the_hub_s_status_keeps_its_form() {
    let status = HubStatus {
        pid: 7,
        hub_version: "0.1.9".into(),
        protocol_version: 3,
        started_at_ms: 1,
        workers: 4,
        running_sessions: 2,
        clients: 1,
        on_demand: true,
        started_by: Some(HubStart::OnDemand),
        build_number: Some(42),
        running_setups: 1,
        requests: 3,
        keeping_awake: true,
        keeps_workers: true,
        oldest_protocol_version: Some(2),
        oldest_running_worker_protocol: Some(3),
        open_terminals: 1,
    };
    answered(
        ClientResponse::HubStatus(status),
        r#"{"result":"hub_status","pid":7,"hub_version":"0.1.9","protocol_version":3,"started_at_ms":1,"workers":4,"running_sessions":2,"clients":1,"on_demand":true,"started_by":"on_demand","build_number":42,"running_setups":1,"requests":3,"keeping_awake":true,"keeps_workers":true,"oldest_protocol_version":2,"oldest_running_worker_protocol":3,"open_terminals":1}"#,
    );
}

/// Its workspace and session are written by today's types, and this module leaves their form to those types' own
/// pins, where they have any.
#[test]
fn what_making_a_workspace_from_the_dialog_made_keeps_its_form() {
    let (workspace, session) = (fixtures::workspace("w"), fixtures::session("s"));
    let records = format!(
        r#""workspace":{},"session":{}"#,
        serde_json::to_string(&workspace).unwrap(),
        serde_json::to_string(&session).unwrap()
    );
    let unsent = UnsentPrompt { error: ApiError::new(ErrorCode::Conflict, "busy"), in_composer: true };
    let made = new_workspace::NewWorkspaceMade { workspace, session, unchosen: None, unsent: Some(unsent) };

    answered(
        ClientResponse::NewWorkspaceMade(Box::new(made)),
        &format!(
            r#"{{"result":"new_workspace_made",{records},"unsent":{{"error":{{"code":"conflict","message":"busy"}},"in_composer":true}}}}"#
        ),
    );
}

#[test]
fn every_reply_about_a_workspace_keeps_its_form() {
    answered(
        ClientResponse::Workspaces(workspaces::Workspaces { workspaces: vec![] }),
        r#"{"result":"workspaces","workspaces":[]}"#,
    );
    answered(
        ClientResponse::SetupLog(workspaces::SetupLog { text: "Running the setup script\n".into() }),
        r#"{"result":"setup_log","text":"Running the setup script\n"}"#,
    );
    answered(
        ClientResponse::PullRequest(workspaces::PullRequestReply { pull_request: None }),
        r#"{"result":"pull_request","pull_request":null}"#,
    );
    let diff = GitDiff { base_commit: Some("c1".into()), patch: String::new(), truncated: false };
    answered(ClientResponse::GitDiff(diff), r#"{"result":"git_diff","base_commit":"c1","patch":"","truncated":false}"#);
    let comment = DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 3,
        text: "why?".into(),
        created_at_ms: 1,
        rev: Rev::new(2),
    };
    let fields =
        r#""id":"c","workspace_id":"w","path":"a.rs","side":"new","line":3,"text":"why?","created_at_ms":1,"rev":2"#;
    answered(ClientResponse::Comment(comment.clone()), &format!(r#"{{"result":"comment",{fields}}}"#));
    answered(
        ClientResponse::Comments(comments::Comments { comments: vec![comment] }),
        &format!(r#"{{"result":"comments","comments":[{{{fields}}}]}}"#),
    );
    let files = WorktreeFiles { paths: vec!["a.rs".into()], folders: vec!["target".into()], truncated: false };
    answered(
        ClientResponse::WorktreeFiles(files),
        r#"{"result":"worktree_files","paths":["a.rs"],"folders":["target"],"truncated":false}"#,
    );
    let text = FileContents::Text { text: "one".into() };
    answered(ClientResponse::FileContents(text), r#"{"result":"file_contents","kind":"text","text":"one"}"#);
    answered(ClientResponse::FileOpened(FileOpened::NotText), r#"{"result":"file_opened","kind":"not_text"}"#);
}

#[test]
fn every_reply_about_a_sessions_log_keeps_its_form() {
    answered(
        ClientResponse::Events(log::Events { events: vec![event()], last_seq: Seq::new(4) }),
        &format!(r#"{{"result":"events","events":[{EVENT}],"last_seq":4}}"#),
    );
    answered(
        ClientResponse::PlacedEvents(log::PlacedEvents { events: vec![event()], last_place: Place::new(3) }),
        &format!(r#"{{"result":"placed_events","events":[{EVENT}],"last_place":3}}"#),
    );
    let line: PromptLine = serde_json::from_str(r#"{"seq":7,"place":6,"text":"Fix it","files":0}"#).unwrap();
    answered(
        ClientResponse::Prompts(log::Prompts { prompts: vec![line] }),
        r#"{"result":"prompts","prompts":[{"seq":7,"place":6,"text":"Fix it","files":0}]}"#,
    );
    let compaction = CompactionLine {
        seq: Seq::new(9),
        place: Place::new(8),
        by: CompactedBy::Asked,
        before_tokens: Some(100),
        after_tokens: None,
    };
    answered(
        ClientResponse::Compactions(log::Compactions { compactions: vec![compaction] }),
        r#"{"result":"compactions","compactions":[{"seq":9,"place":8,"by":"asked","before_tokens":100}]}"#,
    );
    let found = Found { places: vec![Place::new(3)], through: Place::new(4) };
    answered(ClientResponse::Found(found), r#"{"result":"found","places":[3],"through":4}"#);
}

#[test]
fn every_reply_about_a_prompt_or_a_file_keeps_its_form() {
    answered(
        ClientResponse::PromptRecorded(agent::PromptRecorded { seq: Seq::new(3), duplicate: false }),
        r#"{"result":"prompt_recorded","seq":3,"duplicate":false}"#,
    );
    let added = Attachment { id: "a1".into(), name: "build.log".into(), media_type: MediaType::Text, size_bytes: 2 };
    answered(
        ClientResponse::Attachment(added),
        r#"{"result":"attachment","id":"a1","name":"build.log","media_type":"text/plain","size_bytes":2}"#,
    );
    answered(
        ClientResponse::AttachmentData(attachments::AttachmentData { data: AttachmentBytes::new(b"ok".to_vec()) }),
        r#"{"result":"attachment_data","data":"b2s="}"#,
    );
}

/// The name a reply that holds a model type goes by is the reply's own, not the type's, so a variant renamed would fail
/// an older peer though every pin of the type passed.
#[test]
fn every_reply_that_holds_a_model_type_keeps_its_result() {
    let replies = [
        (ClientResponse::Session(fixtures::session("s")), r#""result":"session""#),
        (ClientResponse::Workspace(fixtures::workspace("w")), r#""result":"workspace""#),
        (ClientResponse::GitStatus(GitStatus::default()), r#""result":"git_status""#),
        (
            ClientResponse::HubSettings(HubSettings {
                account_defaults: None,
                keep_awake: true,
                allow_asked_in_full_access: Some(false),
                github: None,
                rev: Rev::new(1),
            }),
            r#""result":"hub_settings""#,
        ),
        (ClientResponse::NewWorkspaceDraft(NewWorkspaceDraft::default()), r#""result":"new_workspace_draft""#),
        (ClientResponse::Records(Records::default()), r#""result":"records""#),
        (ClientResponse::SessionAccount(SessionAccount::default()), r#""result":"session_account""#),
        (ClientResponse::FilePages(FilePages::default()), r#""result":"file_pages""#),
    ];

    for (reply, result) in replies {
        let written = serde_json::to_string(&reply).unwrap();
        assert!(written.starts_with(&format!("{{{result}")), "{written} doesn't start with {result}");
        assert_eq!(serde_json::from_str::<ClientResponse>(&written).unwrap(), reply, "{result} reads back");
    }
}
