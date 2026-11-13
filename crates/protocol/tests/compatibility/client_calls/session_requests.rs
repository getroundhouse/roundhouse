//! Every request about a session, as a client writes it and a hub reads it: its record, its log, its agent, and its
//! files and drafts, with the new-workspace dialog's, and making a workspace from it.

use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::client::{
    ClientRequest, agent, attachments, drafts, log, new_workspace, queue, sessions, subscriptions, waits,
};
use protocol::event::ApprovalDecision;
use protocol::frame;
use protocol::model::{
    AgentKind, AnswerDraft, AnswerSoFar, DraftToKeep, Effort, ModelName, NewWorkspaceDraftToKeep, PermissionMode,
    Place, SessionSettingsChange,
};
use protocol::prompt::Prompt;

use super::asked;

#[test]
fn every_request_about_a_session_keeps_its_form() {
    let options = SessionSettingsChange {
        model: Some(ModelName::named("opus")),
        permission_mode: Some(PermissionMode::Plan),
        effort: None,
    };
    let create = ClientRequest::SessionCreate(sessions::SessionCreate {
        workspace_id: "w".into(),
        agent: AgentKind::Claude,
        options,
        client_request_id: Some("q".into()),
    });
    asked(
        create,
        r#"{"method":"session_create","workspace_id":"w","agent":"claude","model":"opus","permission_mode":"plan","effort":null,"client_request_id":"q"}"#,
    );
    asked(
        ClientRequest::SessionEnsure(sessions::SessionEnsure { workspace_id: "w".into(), agent: AgentKind::Claude }),
        r#"{"method":"session_ensure","workspace_id":"w","agent":"claude"}"#,
    );
    asked(
        ClientRequest::SessionList(sessions::SessionList { workspace_id: "w".into() }),
        r#"{"method":"session_list","workspace_id":"w"}"#,
    );
    asked(ClientRequest::SessionGet(sessions::SessionGet { id: "s".into() }), r#"{"method":"session_get","id":"s"}"#);
    let options = SessionSettingsChange { model: None, permission_mode: None, effort: Some(Effort::High) };
    asked(
        ClientRequest::SessionConfigure(sessions::SessionConfigure { id: "s".into(), options }),
        r#"{"method":"session_configure","id":"s","model":null,"permission_mode":null,"effort":"high"}"#,
    );
    asked(
        ClientRequest::SessionClose(sessions::SessionClose { id: "s".into() }),
        r#"{"method":"session_close","id":"s"}"#,
    );
    asked(
        ClientRequest::SessionReopen(sessions::SessionReopen { id: "s".into() }),
        r#"{"method":"session_reopen","id":"s"}"#,
    );
    asked(
        ClientRequest::SessionMarkRead(sessions::SessionMarkRead { id: "s".into(), through_seq: 9 }),
        r#"{"method":"session_mark_read","id":"s","through_seq":9}"#,
    );
    asked(
        ClientRequest::SessionMarkUnread(sessions::SessionMarkUnread { id: "s".into() }),
        r#"{"method":"session_mark_unread","id":"s"}"#,
    );
    asked(
        ClientRequest::SessionRename(sessions::SessionRename { id: "s".into(), title: "Login".into() }),
        r#"{"method":"session_rename","id":"s","title":"Login"}"#,
    );
    asked(
        ClientRequest::SessionAccount(sessions::SessionAccount { id: "s".into() }),
        r#"{"method":"session_account","id":"s"}"#,
    );
    let call_off = waits::SessionCallOffWait { id: "s".into(), wait: "w1".into() };
    asked(ClientRequest::SessionCallOffWait(call_off), r#"{"method":"session_call_off_wait","id":"s","wait":"w1"}"#);
}

#[test]
fn every_request_that_reads_a_sessions_log_keeps_its_form() {
    let read = ClientRequest::SessionRead(log::SessionRead {
        id: "s".into(),
        from_seq: 1,
        limit: 500,
        max_bytes: None,
        include_raw: false,
    });
    asked(read, r#"{"method":"session_read","id":"s","from_seq":1,"limit":500,"max_bytes":null,"include_raw":false}"#);
    let by_place = ClientRequest::SessionReadPlaces(log::SessionReadPlaces {
        id: "s".into(),
        subagent: Some("toolu_1".into()),
        from_place: Place::new(2),
        limit: 500,
        max_bytes: Some(4096),
        include_raw: true,
    });
    asked(
        by_place,
        r#"{"method":"session_read_places","id":"s","subagent":"toolu_1","from_place":2,"limit":500,"max_bytes":4096,"include_raw":true}"#,
    );
    asked(
        ClientRequest::SessionPromptList(log::SessionPromptList { id: "s".into() }),
        r#"{"method":"session_prompt_list","id":"s"}"#,
    );
    asked(
        ClientRequest::SessionCompactionList(log::SessionCompactionList { id: "s".into() }),
        r#"{"method":"session_compaction_list","id":"s"}"#,
    );
    asked(
        ClientRequest::SessionFind(log::SessionFind { id: "s".into(), subagent: None, query: "login".into() }),
        r#"{"method":"session_find","id":"s","subagent":null,"query":"login"}"#,
    );
}

/// A client that leaves `include_raw` out, as one from before it did, is sent no raw lines.
#[test]
fn a_read_of_a_sessions_log_without_include_raw_includes_none() {
    let read = |form: &str| frame::decode::<ClientRequest>(form.as_bytes()).unwrap();

    let by_seq = read(r#"{"method":"session_read","id":"s","from_seq":1,"limit":500,"max_bytes":null}"#);
    let by_place = read(
        r#"{"method":"session_read_places","id":"s","subagent":null,"from_place":1,"limit":500,"max_bytes":null}"#,
    );

    assert!(matches!(by_seq, ClientRequest::SessionRead(log::SessionRead { include_raw: false, .. })), "{by_seq:?}");
    let ClientRequest::SessionReadPlaces(log::SessionReadPlaces { include_raw, .. }) = by_place else {
        panic!("{by_place:?}")
    };
    assert!(!include_raw);
}

#[test]
fn every_request_that_follows_a_sessions_log_or_the_feed_keeps_its_form() {
    asked(
        ClientRequest::SessionSubscribe(subscriptions::SessionSubscribe {
            id: "s".into(),
            from_seq: 5,
            subscription_id: 3,
        }),
        r#"{"method":"session_subscribe","id":"s","from_seq":5,"subscription_id":3}"#,
    );
    let follow = ClientRequest::SessionSubscribePlaces(subscriptions::SessionSubscribePlaces {
        id: "s".into(),
        subagent: None,
        from_place: Place::new(1),
        subscription_id: 4,
    });
    asked(
        follow,
        r#"{"method":"session_subscribe_places","id":"s","subagent":null,"from_place":1,"subscription_id":4}"#,
    );
    asked(
        ClientRequest::SessionUnsubscribe(subscriptions::SessionUnsubscribe { subscription_id: 3 }),
        r#"{"method":"session_unsubscribe","subscription_id":3}"#,
    );
    asked(
        ClientRequest::FeedSubscribe(subscriptions::FeedSubscribe { subscription_id: 2 }),
        r#"{"method":"feed_subscribe","subscription_id":2}"#,
    );
}

#[test]
fn every_request_for_a_sessions_agent_keeps_its_form() {
    let prompt = ClientRequest::SessionPrompt(agent::SessionPrompt {
        id: "s".into(),
        client_msg_id: Some("m".into()),
        prompt: Prompt::new("go"),
    });
    asked(prompt, r#"{"method":"session_prompt","id":"s","client_msg_id":"m","text":"go"}"#);
    let decision = ApprovalDecision::Deny { message: None, interrupt: true };
    asked(
        ClientRequest::SessionApprove(agent::SessionApprove { id: "s".into(), approval_id: "a".into(), decision }),
        r#"{"method":"session_approve","id":"s","approval_id":"a","decision":{"decision":"deny","interrupt":true}}"#,
    );
    asked(
        ClientRequest::SessionCancel(agent::SessionCancel { id: "s".into() }),
        r#"{"method":"session_cancel","id":"s"}"#,
    );
    asked(
        ClientRequest::SessionCompact(agent::SessionCompact { id: "s".into(), keeping: None }),
        r#"{"method":"session_compact","id":"s"}"#,
    );
    asked(
        ClientRequest::SessionCompact(agent::SessionCompact { id: "s".into(), keeping: Some("The API".into()) }),
        r#"{"method":"session_compact","id":"s","keeping":"The API"}"#,
    );
    let queue = queue::SessionQueue { id: "s".into(), client_msg_id: "m".into(), prompt: Prompt::new("next") };
    asked(
        ClientRequest::SessionQueue(queue),
        r#"{"method":"session_queue","id":"s","client_msg_id":"m","text":"next"}"#,
    );
    let change = queue::SessionChangeQueued { id: "s".into(), client_msg_id: "m".into(), text: "then".into() };
    asked(
        ClientRequest::SessionChangeQueued(change),
        r#"{"method":"session_change_queued","id":"s","client_msg_id":"m","text":"then"}"#,
    );
    asked(
        ClientRequest::SessionUnqueue(queue::SessionUnqueue { id: "s".into(), client_msg_id: "m".into() }),
        r#"{"method":"session_unqueue","id":"s","client_msg_id":"m"}"#,
    );
    asked(
        ClientRequest::SessionSendQueued(queue::SessionSendQueued { id: "s".into(), client_msg_id: "m".into() }),
        r#"{"method":"session_send_queued","id":"s","client_msg_id":"m"}"#,
    );
}

#[test]
fn every_request_about_files_and_drafts_keeps_its_form() {
    let data = || AttachmentBytes::new(b"ok".to_vec());
    let add = ClientRequest::AttachmentAdd(attachments::AttachmentAdd {
        session_id: "s".into(),
        id: "a1".into(),
        name: "build.log".into(),
        media_type: MediaType::Text,
        data: data(),
    });
    asked(
        add,
        r#"{"method":"attachment_add","session_id":"s","id":"a1","name":"build.log","media_type":"text/plain","data":"b2s="}"#,
    );
    asked(
        ClientRequest::AttachmentRead(attachments::AttachmentRead { session_id: "s".into(), id: "a1".into() }),
        r#"{"method":"attachment_read","session_id":"s","id":"a1"}"#,
    );
    asked(
        ClientRequest::AttachmentRemove(attachments::AttachmentRemove { session_id: "s".into(), id: "a1".into() }),
        r#"{"method":"attachment_remove","session_id":"s","id":"a1"}"#,
    );
    asked(
        ClientRequest::SessionDraft(drafts::SessionDraft { id: "s".into(), draft: DraftToKeep::new("half") }),
        r#"{"method":"session_draft","id":"s","draft":{"text":"half"}}"#,
    );
    let answer =
        AnswerDraft { approval_id: "a".into(), answers: vec![AnswerSoFar { picked: vec![1], ..Default::default() }] };
    asked(
        ClientRequest::SessionAnswerDraft(drafts::SessionAnswerDraft { id: "s".into(), draft: Some(answer) }),
        r#"{"method":"session_answer_draft","id":"s","draft":{"approval_id":"a","answers":[{"picked":[1]}]}}"#,
    );
    asked(
        ClientRequest::SessionAnswerDraft(drafts::SessionAnswerDraft { id: "s".into(), draft: None }),
        r#"{"method":"session_answer_draft","id":"s","draft":null}"#,
    );
}

#[test]
fn every_request_about_the_new_workspace_dialog_keeps_its_form() {
    let dialog =
        || NewWorkspaceDraftToKeep { text: "fix the login".into(), files: vec!["f1".into()], ..Default::default() };
    asked(
        ClientRequest::NewWorkspaceDraftKeep(drafts::NewWorkspaceDraftKeep { draft: dialog() }),
        r#"{"method":"new_workspace_draft_keep","draft":{"text":"fix the login","files":["f1"]}}"#,
    );
    let file = drafts::NewWorkspaceDraftFileAdd {
        id: "f1".into(),
        name: "notes.txt".into(),
        media_type: MediaType::Text,
        data: AttachmentBytes::new(b"ok".to_vec()),
    };
    asked(
        ClientRequest::NewWorkspaceDraftFileAdd(file),
        r#"{"method":"new_workspace_draft_file_add","id":"f1","name":"notes.txt","media_type":"text/plain","data":"b2s="}"#,
    );
    asked(
        ClientRequest::NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead { id: "f1".into() }),
        r#"{"method":"new_workspace_draft_file_read","id":"f1"}"#,
    );
    asked(
        ClientRequest::NewWorkspaceDraftFileRemove(drafts::NewWorkspaceDraftFileRemove { id: "f1".into() }),
        r#"{"method":"new_workspace_draft_file_remove","id":"f1"}"#,
    );
    let create = new_workspace::NewWorkspaceCreate { made_of: dialog(), client_request_id: "make it".into() };
    asked(
        ClientRequest::NewWorkspaceCreate(create),
        r#"{"method":"new_workspace_create","made_of":{"text":"fix the login","files":["f1"]},"client_request_id":"make it"}"#,
    );
}
