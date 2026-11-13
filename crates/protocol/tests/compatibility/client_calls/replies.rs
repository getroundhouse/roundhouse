//! Every reply, as a hub writes it and a client reads it, each with at least its `result`. Most that hold a model type
//! are spelled out whole. The rest, such as a session's record or a git status, are that type's form beside their
//! `result`, so only their `result` is pinned here, and the type's form is left to its own pins, where it has any.

use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::client::new_workspace::UnsentPrompt;
use protocol::client::{
    ApiError, ClientResponse, ErrorCode, HelloReply, HubStatus, admin, agent, attachments, comments, log,
    new_workspace, records, sessions, workspaces,
};
use protocol::event::{CompactedBy, SessionEvent};
use protocol::find::Found;
use protocol::fixtures;
use protocol::model::{
    Account, CompactionLine, DiffComment, DiffSide, FileContents, FileOpened, FilePages, GitDiff, GitStatus,
    HubSettings, NewWorkspaceDraft, Place, PromptLine, Records, Repo, Rev, Seq, SessionAccount, WorktreeFiles,
};

use super::answered;

/// One of the agent's own events, which is where it sits in a reply's `events`.
const EVENT: &str =
    r#"{"session_id":"s","seq":4,"ts_ms":1,"place":3,"body":{"type":"tool_call_dropped","tool_use_id":"toolu_1"}}"#;

fn event() -> SessionEvent {
    serde_json::from_str(EVENT).unwrap()
}

#[test]
fn a_hubs_settings_keep_their_form_and_one_from_a_hub_from_before_says_nothing_of_allowing_tools() {
    let settings = HubSettings { keep_awake: false, allow_asked_in_full_access: Some(true), rev: Rev::new(3) };
    answered(
        ClientResponse::HubSettings(settings),
        r#"{"result":"hub_settings","keep_awake":false,"allow_asked_in_full_access":true,"rev":3}"#,
    );
    // From a hub from before it could allow a tool's use itself, which says nothing of it.
    let older: ClientResponse = serde_json::from_str(r#"{"result":"hub_settings","keep_awake":true,"rev":3}"#).unwrap();
    let settings = HubSettings { keep_awake: true, allow_asked_in_full_access: None, rev: Rev::new(3) };
    assert_eq!(older, ClientResponse::HubSettings(settings));
}

#[test]
fn every_reply_of_the_hub_itself_or_listing_its_records_keeps_its_form() {
    answered(ClientResponse::Ok, r#"{"result":"ok"}"#);
    answered(
        ClientResponse::Hello(HelloReply { protocol_version: 3, hub_version: "0.1.9".into() }),
        r#"{"result":"hello","protocol_version":3,"hub_version":"0.1.9"}"#,
    );
    let status = HubStatus {
        pid: 7,
        hub_version: "0.1.9".into(),
        protocol_version: 3,
        started_at_ms: 1,
        workers: 4,
        running_sessions: 2,
        clients: 1,
        on_demand: true,
        build_number: Some(42),
        running_setups: 1,
        requests: 3,
        keeping_awake: true,
        keeps_workers: true,
        oldest_protocol_version: Some(2),
    };
    answered(
        ClientResponse::HubStatus(status),
        r#"{"result":"hub_status","pid":7,"hub_version":"0.1.9","protocol_version":3,"started_at_ms":1,"workers":4,"running_sessions":2,"clients":1,"on_demand":true,"build_number":42,"running_setups":1,"requests":3,"keeping_awake":true,"keeps_workers":true,"oldest_protocol_version":2}"#,
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
        rev: Rev::new(2),
    };
    answered(
        ClientResponse::Account(account),
        r#"{"result":"account","id":"claude-1","model_provider":null,"plan":null,"login":null,"email":null,"name":"Work","login_agent":null,"limits":[],"held_login":null,"rev":2}"#,
    );
    let repo = Repo { id: "r".into(), name: "r".into(), path: "/r".into(), created_at_ms: 1, rev: Rev::new(2) };
    answered(
        ClientResponse::Repo(repo.clone()),
        r#"{"result":"repo","id":"r","name":"r","path":"/r","created_at_ms":1,"rev":2}"#,
    );
    answered(
        ClientResponse::Repos(records::Repos { repos: vec![repo] }),
        r#"{"result":"repos","repos":[{"id":"r","name":"r","path":"/r","created_at_ms":1,"rev":2}]}"#,
    );
    answered(
        ClientResponse::Sessions(sessions::Sessions { sessions: vec![] }),
        r#"{"result":"sessions","sessions":[]}"#,
    );
    answered(
        ClientResponse::Subagents(records::Subagents { subagents: vec![], rev: Rev::new(9) }),
        r#"{"result":"subagents","subagents":[],"rev":9}"#,
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
        (ClientResponse::Session(fixtures::session("s")), "session"),
        (ClientResponse::Workspace(fixtures::workspace("w")), "workspace"),
        (ClientResponse::GitStatus(GitStatus::default()), "git_status"),
        (
            ClientResponse::HubSettings(HubSettings {
                keep_awake: true,
                allow_asked_in_full_access: Some(false),
                rev: Rev::new(1),
            }),
            "hub_settings",
        ),
        (ClientResponse::NewWorkspaceDraft(NewWorkspaceDraft::default()), "new_workspace_draft"),
        (ClientResponse::Records(Records::default()), "records"),
        (ClientResponse::SessionAccount(SessionAccount::default()), "session_account"),
        (ClientResponse::FilePages(FilePages::default()), "file_pages"),
    ];

    for (reply, result) in replies {
        let written = serde_json::to_value(&reply).unwrap();
        assert_eq!(written["result"], result, "{reply:?}");
        assert_eq!(serde_json::from_value::<ClientResponse>(written).unwrap(), reply, "{result} reads back");
    }
}
