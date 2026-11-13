//! Every request of the hub itself, for its records or its agents' logins, or about a workspace, as a client writes it
//! and a hub reads it.

use protocol::client::{ClientRequest, Hello, accounts, admin, comments, drafts, files, records, workspaces};
use protocol::frame;
use protocol::model::{AgentKind, CommentDraft, DiffSide, HubSettingsChange, LoginToken};

use super::asked;

#[test]
fn every_request_of_the_hub_itself_or_for_its_records_keeps_its_form() {
    let ranges =
        ClientRequest::Hello(Hello { protocol_version: 4, client: "c".into(), oldest_protocol_version: Some(3) });
    asked(ranges, r#"{"method":"hello","protocol_version":4,"client":"c","oldest_protocol_version":3}"#);
    asked(ClientRequest::HubStatus, r#"{"method":"hub_status"}"#);
    asked(ClientRequest::HubShutdown, r#"{"method":"hub_shutdown"}"#);
    asked(ClientRequest::HubShutdownIfIdle, r#"{"method":"hub_shutdown_if_idle"}"#);
    asked(ClientRequest::HubRestart, r#"{"method":"hub_restart"}"#);
    let settings = HubSettingsChange { keep_awake: Some(false) };
    asked(
        ClientRequest::HubConfigure(admin::HubConfigure { settings }),
        r#"{"method":"hub_configure","keep_awake":false}"#,
    );
    let settings = HubSettingsChange { keep_awake: None };
    asked(ClientRequest::HubConfigure(admin::HubConfigure { settings }), r#"{"method":"hub_configure"}"#);
    asked(ClientRequest::AgentList, r#"{"method":"agent_list"}"#);
    asked(ClientRequest::RepoAdd(records::RepoAdd { path: "/r".into() }), r#"{"method":"repo_add","path":"/r"}"#);
    asked(ClientRequest::RepoList, r#"{"method":"repo_list"}"#);
    asked(ClientRequest::RecordsList, r#"{"method":"records_list"}"#);
    asked(
        ClientRequest::SessionSubagents(records::SessionSubagents { id: "s".into() }),
        r#"{"method":"session_subagents","id":"s"}"#,
    );
}

#[test]
fn every_request_about_an_agents_login_keeps_its_form() {
    let connect = |account: Option<&str>| {
        ClientRequest::AccountConnect(accounts::AccountConnect {
            agent: AgentKind::Claude,
            name: "Work".into(),
            token: LoginToken::new("a-made-up-token"),
            account: account.map(Into::into),
        })
    };
    asked(connect(None), r#"{"method":"account_connect","agent":"claude","name":"Work","token":"a-made-up-token"}"#);
    asked(
        connect(Some("claude-1")),
        r#"{"method":"account_connect","agent":"claude","name":"Work","token":"a-made-up-token","account":"claude-1"}"#,
    );
    asked(
        ClientRequest::AccountDisconnect(accounts::AccountDisconnect { id: "claude-1".into() }),
        r#"{"method":"account_disconnect","id":"claude-1"}"#,
    );
}

#[test]
fn every_request_about_a_workspace_keeps_its_form() {
    let create = ClientRequest::WorkspaceCreate(workspaces::WorkspaceCreate {
        repo_id: "r".into(),
        name: Some("aspen".into()),
        base_ref: None,
        client_request_id: Some("q".into()),
    });
    asked(
        create,
        r#"{"method":"workspace_create","repo_id":"r","name":"aspen","base_ref":null,"client_request_id":"q"}"#,
    );
    asked(
        ClientRequest::WorkspaceList(workspaces::WorkspaceList { include_archived: true }),
        r#"{"method":"workspace_list","include_archived":true}"#,
    );
    asked(
        ClientRequest::WorkspaceArchive(workspaces::WorkspaceArchive { id: "w".into(), force: false }),
        r#"{"method":"workspace_archive","id":"w","force":false}"#,
    );
    asked(
        ClientRequest::WorkspaceRestore(workspaces::WorkspaceRestore { id: "w".into() }),
        r#"{"method":"workspace_restore","id":"w"}"#,
    );
    asked(
        ClientRequest::WorkspaceRename(workspaces::WorkspaceRename { id: "w".into(), title: "Login".into() }),
        r#"{"method":"workspace_rename","id":"w","title":"Login"}"#,
    );
    asked(
        ClientRequest::WorkspaceRegenerateTitle(workspaces::WorkspaceRegenerateTitle { id: "w".into() }),
        r#"{"method":"workspace_regenerate_title","id":"w"}"#,
    );
    asked(
        ClientRequest::WorkspaceStatus(workspaces::WorkspaceStatus { id: "w".into() }),
        r#"{"method":"workspace_status","id":"w"}"#,
    );
    asked(
        ClientRequest::WorkspaceDiff(workspaces::WorkspaceDiff { id: "w".into() }),
        r#"{"method":"workspace_diff","id":"w"}"#,
    );
    asked(
        ClientRequest::WorkspaceSetupLog(workspaces::WorkspaceSetupLog { id: "w".into() }),
        r#"{"method":"workspace_setup_log","id":"w"}"#,
    );
    asked(
        ClientRequest::WorkspaceRerunSetup(workspaces::WorkspaceRerunSetup { id: "w".into() }),
        r#"{"method":"workspace_rerun_setup","id":"w"}"#,
    );
    asked(
        ClientRequest::WorkspacePullRequest(workspaces::WorkspacePullRequest { id: "w".into() }),
        r#"{"method":"workspace_pull_request","id":"w"}"#,
    );
    let comment =
        CommentDraft { path: "a.rs".into(), side: DiffSide::New, line: 3, editing: None, text: "why?".into() };
    asked(
        ClientRequest::WorkspaceCommentDraft(drafts::WorkspaceCommentDraft { id: "w".into(), draft: Some(comment) }),
        r#"{"method":"workspace_comment_draft","id":"w","draft":{"path":"a.rs","side":"new","line":3,"text":"why?"}}"#,
    );
}

#[test]
fn every_request_about_a_comment_keeps_its_form() {
    let add = ClientRequest::CommentAdd(comments::CommentAdd {
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::Old,
        line: 3,
        text: "why?".into(),
        client_request_id: None,
    });
    asked(
        add,
        r#"{"method":"comment_add","workspace_id":"w","path":"a.rs","side":"old","line":3,"text":"why?","client_request_id":null}"#,
    );
    asked(
        ClientRequest::CommentList(comments::CommentList { workspace_id: "w".into() }),
        r#"{"method":"comment_list","workspace_id":"w"}"#,
    );
    asked(
        ClientRequest::CommentEdit(comments::CommentEdit { id: "c".into(), text: "why not?".into() }),
        r#"{"method":"comment_edit","id":"c","text":"why not?"}"#,
    );
    asked(
        ClientRequest::CommentDelete(comments::CommentDelete { id: "c".into() }),
        r#"{"method":"comment_delete","id":"c"}"#,
    );
}

#[test]
fn every_request_about_what_a_workspace_holds_keeps_its_form() {
    asked(
        ClientRequest::WorkspaceFiles(files::WorkspaceFiles { id: "w".into() }),
        r#"{"method":"workspace_files","id":"w"}"#,
    );
    asked(
        ClientRequest::WorkspaceFolderRead(files::WorkspaceFolderRead { id: "w".into(), path: "src".into() }),
        r#"{"method":"workspace_folder_read","id":"w","path":"src"}"#,
    );
    asked(
        ClientRequest::WorkspaceFileRead(files::WorkspaceFileRead { id: "w".into(), path: "a.rs".into() }),
        r#"{"method":"workspace_file_read","id":"w","path":"a.rs"}"#,
    );
    let at = files::WorkspaceFileReadAt { id: "w".into(), commit: "c1".into(), path: "a.rs".into() };
    asked(
        ClientRequest::WorkspaceFileReadAt(at),
        r#"{"method":"workspace_file_read_at","id":"w","commit":"c1","path":"a.rs"}"#,
    );
    let open = files::WorkspaceFileOpenAt { id: "w".into(), commit: "c1".into(), path: "a.rs".into() };
    asked(
        ClientRequest::WorkspaceFileOpenAt(open),
        r#"{"method":"workspace_file_open_at","id":"w","commit":"c1","path":"a.rs"}"#,
    );
    let pages = files::WorkspaceFilePages {
        id: "w".into(),
        commit: "c1".into(),
        path: "a.rs".into(),
        page_lines: 256,
        pages: vec![0, 3],
    };
    asked(
        ClientRequest::WorkspaceFilePages(pages),
        r#"{"method":"workspace_file_pages","id":"w","commit":"c1","path":"a.rs","page_lines":256,"pages":[0,3]}"#,
    );
}

/// A unit request reads with fields a newer client added to it, as any request does.
#[test]
fn a_request_with_no_fields_reads_with_fields_it_doesnt_know() {
    let read = |form: &str| frame::decode::<ClientRequest>(form.as_bytes()).unwrap();

    assert_eq!(read(r#"{"method":"hub_status","newer":1}"#), ClientRequest::HubStatus);
    assert_eq!(read(r#"{"method":"hub_shutdown","newer":1}"#), ClientRequest::HubShutdown);
    assert_eq!(read(r#"{"method":"hub_shutdown_if_idle","newer":1}"#), ClientRequest::HubShutdownIfIdle);
    assert_eq!(read(r#"{"method":"hub_restart","newer":1}"#), ClientRequest::HubRestart);
    assert_eq!(read(r#"{"method":"agent_list","newer":1}"#), ClientRequest::AgentList);
    assert_eq!(read(r#"{"method":"repo_list","newer":1}"#), ClientRequest::RepoList);
    assert_eq!(read(r#"{"method":"records_list","newer":1}"#), ClientRequest::RecordsList);
}
