use protocol::attachment::{Attachment, AttachmentBytes, MediaType};
use protocol::capability::settings::Effort;
use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::model::{
    AgentDescription, AgentKind, AttachmentId, ModelName, NewWorkspaceDraft, NewWorkspaceDraftToKeep, PermissionMode,
    RepoId, Seq, SessionId,
};
use protocol::prompt::Prompt;
use protocol::worker::WorkerRequest;

use crate::attachment_files::FileFor;
use crate::hub::Hub;
use crate::hub::stand_in_workers::{TIMEOUT, hub_whose_stand_ins_answer_first, hub_with_stand_in_workers};
use crate::hub::tests::{described, hub_in};
use crate::store::AddedBy;

fn in_the_repo(files: &[&str]) -> NewWorkspaceDraftToKeep {
    NewWorkspaceDraftToKeep {
        repo_id: Some(RepoId::from("r")),
        text: "fix the login".to_string(),
        files: files.iter().map(|&id| id.into()).collect(),
        ..NewWorkspaceDraftToKeep::default()
    }
}

/// Keeps `name` for the dialog as the file `id`, which says `said`.
async fn added_to_the_dialog(hub: &Hub, id: &str, name: &str, said: &[u8]) {
    let data = AttachmentBytes::new(said.to_vec());
    hub.file_add(FileFor::NewWorkspace, id.into(), name.into(), MediaType::Text, data, AddedBy::Client).await.unwrap();
}

/// Keeps a task in the dialog, with the file `notes`, for the repository `r`, planning on Sonnet at low effort, which
/// isn't how a chat nobody has chosen for runs.
async fn kept_in_the_dialog(hub: &Hub) -> NewWorkspaceDraftToKeep {
    added_to_the_dialog(hub, "notes", "notes.txt", b"one\n").await;
    let draft = NewWorkspaceDraftToKeep {
        model: Some(ModelName::named("sonnet")),
        effort: Some(Effort::Low),
        plan: true,
        ..in_the_repo(&["notes"])
    };
    hub.new_workspace_draft_keep(draft.clone()).await.unwrap();
    draft
}

/// Each prompt the chat has been sent: what it said, and the files it carried.
fn prompts(hub: &Hub, session: &SessionId) -> Vec<Prompt<Attachment>> {
    let events = hub.store.read_events(session, Seq::FIRST, 100, u64::MAX, false).unwrap();
    let prompt = |body| if let EventBody::UserPrompt { prompt, .. } = body { Some(prompt) } else { None };
    events.into_iter().filter_map(|event| prompt(event.body)).collect()
}

/// What's in the dialog, as the hub keeps it.
async fn in_the_dialog(hub: &Hub) -> NewWorkspaceDraft {
    hub.records_list().await.unwrap().new_workspace_draft.unwrap()
}

/// The dialog's file `id`, or why the hub can't read it.
async fn dialogs_file(hub: &Hub, id: &str) -> Result<Vec<u8>, ErrorCode> {
    hub.file_read(FileFor::NewWorkspace, id.into()).await.map(AttachmentBytes::into_vec).map_err(|e| e.code)
}

/// Its agent starts in the worktree its worker made, once the worker has put the prompt's file there.
#[tokio::test]
async fn a_workspace_made_from_the_dialog_has_what_it_said_and_the_dialog_is_emptied() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let made_of = kept_in_the_dialog(&hub).await;

    let made = hub.new_workspace_create(made_of.clone(), "make it".into()).await.unwrap();

    let (workspace, session) = (&made.workspace, &made.session);
    assert_eq!((made.unchosen, made.unsent), (None, None));
    assert_eq!((&session.workspace_id, &workspace.repo_id), (&workspace.id, made_of.repo_id.as_ref().unwrap()));
    assert_eq!(
        (&session.model, session.effort.clone(), session.permission_mode),
        (&ModelName::named("sonnet"), Effort::Low, PermissionMode::Plan)
    );
    let [Prompt { text, files }] = prompts(&hub, &session.id).try_into().unwrap();
    assert_eq!(text, "fix the login");
    assert_eq!(
        files.iter().map(|file| (file.id.as_str(), file.name.as_str())).collect::<Vec<_>>(),
        [("notes", "notes.txt")]
    );
    let kept = in_the_dialog(&hub).await;
    assert_eq!((kept.text.as_str(), kept.files.as_slice(), kept.plan), ("", [].as_slice(), false));
    assert_eq!(
        (kept.repo_id, kept.model, kept.effort),
        (made_of.repo_id, made_of.model, made_of.effort),
        "the next one's made the same way"
    );
    assert_eq!(dialogs_file(&hub, "notes").await, Err(ErrorCode::NotFound), "the dialog's copy is let go of");
    let worktree = workspace.path.clone().unwrap();
    let launched = || asked.requests(started_in).pop().ok_or("its agent was never started");
    assert_eq!(waiting::task::until_ok(TIMEOUT, launched).await, worktree);
    assert_eq!(asked.requests(written_to), [(worktree, "notes.txt".to_string(), b"one\n".to_vec())]);
}

/// Where a worker was asked to start an agent.
fn started_in(request: &WorkerRequest) -> Option<String> {
    let WorkerRequest::AgentStart(spec) = request else { return None };
    Some(spec.cwd.clone())
}

/// Where a worker was asked to write a file a prompt carries, the file's name, and what it says.
fn written_to(request: &WorkerRequest) -> Option<(String, String, Vec<u8>)> {
    let WorkerRequest::AttachmentWrite(write) = request else { return None };
    Some((write.path.clone(), write.name.clone(), write.data.as_slice().to_vec()))
}

/// Such as in another client, as the workspace was being made.
#[tokio::test]
async fn whats_typed_in_the_dialog_since_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _asked) = hub_with_stand_in_workers(dir.path());
    let made_of = kept_in_the_dialog(&hub).await;
    let since = NewWorkspaceDraftToKeep { text: "fix the logout".to_string(), ..made_of.clone() };
    hub.new_workspace_draft_keep(since).await.unwrap();

    let made = hub.new_workspace_create(made_of, "make it".into()).await.unwrap();

    assert_eq!(prompts(&hub, &made.session.id)[0].text, "fix the login");
    assert_eq!(in_the_dialog(&hub).await.text, "fix the logout");
    assert_eq!(dialogs_file(&hub, "notes").await.as_deref(), Ok(b"one\n".as_slice()), "it still names its file");
}

#[tokio::test]
async fn sent_again_it_makes_nothing_more() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let made_of = kept_in_the_dialog(&hub).await;
    let before = hub.workspace_list(true).await.unwrap().len();
    let first = hub.new_workspace_create(made_of.clone(), "make it".into()).await.unwrap();

    let again = hub.new_workspace_create(made_of, "make it".into()).await.unwrap();

    assert_eq!((&again.workspace.id, &again.session.id), (&first.workspace.id, &first.session.id));
    assert_eq!(again.unsent, None);
    assert_eq!(prompts(&hub, &first.session.id).len(), 1);
    assert_eq!(hub.workspace_list(true).await.unwrap().len(), before + 1);
    let worktrees = asked.requests(|request| matches!(request, WorkerRequest::PrepareWorkspace(_)).then_some(()));
    assert_eq!(worktrees.len(), 1, "its worker was asked for one worktree");
}

#[tokio::test]
async fn with_nothing_typed_the_workspace_and_its_chat_are_made_and_nothing_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _asked) = hub_with_stand_in_workers(dir.path());
    let made_of = NewWorkspaceDraftToKeep { text: String::new(), files: Vec::new(), ..kept_in_the_dialog(&hub).await };
    hub.new_workspace_draft_keep(made_of.clone()).await.unwrap();

    let made = hub.new_workspace_create(made_of, "make it".into()).await.unwrap();

    assert_eq!(made.session.permission_mode, PermissionMode::Plan);
    assert_eq!(made.unsent, None);
    assert_eq!(prompts(&hub, &made.session.id), []);
    assert!(!in_the_dialog(&hub).await.plan, "the dialog is emptied, plan mode too");
}

/// So a chat that's to plan doesn't start on it with full access. Here the effort is one Claude doesn't offer, as this
/// hub describes it.
#[tokio::test]
async fn a_prompt_for_a_chat_that_couldnt_be_given_what_was_picked_goes_to_its_composer_with_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _asked) = hub_with_stand_in_workers(dir.path());
    let made_of = NewWorkspaceDraftToKeep { effort: Some(Effort::Medium), ..kept_in_the_dialog(&hub).await };
    hub.new_workspace_draft_keep(made_of.clone()).await.unwrap();

    let made = hub.new_workspace_create(made_of, "make it".into()).await.unwrap();

    let unchosen = made.unchosen.map(|unchosen| unchosen.code);
    assert_eq!(unchosen, Some(ErrorCode::InvalidRequest), "it says the chat couldn't be given what was picked");
    let unsent = made.unsent.map(|unsent| (unsent.error.code, unsent.in_composer));
    assert_eq!(unsent, Some((ErrorCode::InvalidRequest, true)));
    let draft = &made.session.draft;
    assert_eq!(draft.text, "fix the login");
    assert_eq!(draft.files.iter().map(|file| file.id.as_str()).collect::<Vec<_>>(), ["notes"]);
    assert_eq!(prompts(&hub, &made.session.id), []);
    assert_eq!(in_the_dialog(&hub).await.text, "", "the chat has it now");
}

/// The prompt goes to the chat only whole. Here the hub has lost a file's bytes, so it can't be copied, and the prompt
/// stays in the dialog, which keeps every file, rather than going to the composer without the one that couldn't.
#[tokio::test]
async fn a_prompt_whose_file_cant_be_copied_stays_in_the_dialog_with_every_file() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _asked) = hub_with_stand_in_workers(dir.path());
    added_to_the_dialog(&hub, "lost", "lost.txt", b"two\n").await;
    let made_of =
        NewWorkspaceDraftToKeep { files: vec!["notes".into(), "lost".into()], ..kept_in_the_dialog(&hub).await };
    hub.new_workspace_draft_keep(made_of.clone()).await.unwrap();
    std::fs::remove_file(dir.path().join("attachments").join("workspace-draft.files").join("lost")).unwrap();

    let made = hub.new_workspace_create(made_of, "make it".into()).await.unwrap();

    let unsent = made.unsent.map(|unsent| (unsent.error.code, unsent.in_composer));
    assert_eq!(unsent, Some((ErrorCode::NotFound, false)));
    assert!(made.session.draft.is_empty(), "nothing went to the composer");
    assert_eq!(prompts(&hub, &made.session.id), []);
    let kept = in_the_dialog(&hub).await;
    assert_eq!(kept.text, "fix the login", "the dialog still has it");
    assert_eq!(kept.files.iter().map(|file| file.id.as_str()).collect::<Vec<_>>(), ["notes", "lost"]);
    assert_eq!(dialogs_file(&hub, "notes").await.as_deref(), Ok(b"one\n".as_slice()));
}

/// Each is refused before a workspace is made, so a client can give it back to the dialog, which loses nothing, and
/// leaves no workspace behind without its prompt.
#[tokio::test]
async fn what_cant_be_made_is_refused_before_anything_is() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let before = hub.workspace_list(true).await.unwrap();
    let nowhere = NewWorkspaceDraftToKeep { repo_id: None, ..in_the_repo(&[]) };
    let too_many: Vec<String> = (0..=protocol::attachment::MAX_PER_PROMPT).map(|n| format!("f{n}")).collect();
    let too_many = in_the_repo(&too_many.iter().map(String::as_str).collect::<Vec<_>>());
    let refusals = [
        (nowhere, ErrorCode::InvalidRequest),
        (too_many, ErrorCode::InvalidRequest),
        (in_the_repo(&["not-the-dialogs"]), ErrorCode::NotFound),
    ];

    for (n, (made_of, code)) in refusals.into_iter().enumerate() {
        let refused = hub.new_workspace_create(made_of, format!("make {n}").into()).await;

        assert_eq!(refused.map_err(|e| e.code).err(), Some(code), "the request {n}");
    }
    assert_eq!(hub.workspace_list(true).await.unwrap(), before, "no workspace was made");
}

/// A file the dialog has had for long isn't let go of while a workspace is made from it, should the dialog stop naming
/// it meanwhile. It waits as long again once the hub has found it, even if making the workspace then fails, as it does
/// here, since this hub never starts a worker.
#[tokio::test]
async fn the_dialogs_files_wait_as_long_again_once_a_workspace_is_made_from_them() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    added_to_the_dialog(&hub, "f1", "notes.txt", b"a line\n").await;
    // Added at the start of the clock, so long before it's asked for, by any clock.
    hub.store.holding_writes().execute("UPDATE new_workspace_draft_files SET created_at_ms = 0", []).unwrap();

    let _ = hub.new_workspace_create(in_the_repo(&["f1"]), "make".into()).await;

    let let_go_of = hub.blocking(|store| store.let_go_of_unnamed_new_workspace_draft_files(1)).await;
    assert_eq!(let_go_of.unwrap(), Vec::<AttachmentId>::new(), "added before it was asked for, and renewed since");
}

/// Such as by a client showing the workspace once it's ready, before the hub has made the chat itself, which it then
/// finds, as it would one of the agent asked for.
#[tokio::test]
async fn a_first_chat_asked_for_as_the_workspace_is_made_is_the_one_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let other = AgentDescription { kind: AgentKind::Codex, name: "Codex".to_string(), ..described() };
    let (hub, _asked) = hub_whose_stand_ins_answer_first(dir.path(), vec![described(), other], |_| None);
    let workspace = hub.make_workspace("r".into(), None, None, None, Some(AgentKind::Codex)).await.unwrap();

    let asked_for = hub.session_ensure(workspace.id.clone(), AgentKind::Claude).await.unwrap();

    assert_eq!(asked_for.agent, AgentKind::Codex);
    let made_of = NewWorkspaceDraftToKeep { agent: Some(AgentKind::Codex), ..in_the_repo(&[]) };
    let (first, unchosen) = hub.first_chat(&workspace.id, &made_of).await.unwrap();
    assert_eq!((first.id, unchosen), (asked_for.id, None), "the hub's is the one asked for");
    let next = hub.make_workspace("r".into(), None, None, None, None).await.unwrap();
    let anyones = hub.session_ensure(next.id, AgentKind::Claude).await.unwrap();
    assert_eq!(anyones.agent, AgentKind::Claude, "a workspace made otherwise runs what's asked for");
}
