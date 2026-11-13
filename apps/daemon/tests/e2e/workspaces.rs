//! Workspaces.

#![cfg(test)]

use std::path::PathBuf;

use client_core::{ClientError, new_request_id};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::EventBody;
use protocol::model::{
    AgentKind, ClientRequestId, DiffSide, FileContents, FileOpened, FilePage, PageLine, SessionSettingsChange,
    SessionStatus, WorkspaceStatus,
};

use crate::support::{
    Stack, assistant_text, commit_settings, git, is_alive, next_notice, read_all, wait_for_setup, wait_for_turn_end,
    wait_until,
};

#[test]
fn archiving_protects_uncommitted_work_keeps_the_branch_and_restoring_brings_it_all_back() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());
    client.session_prompt(&session.id, "tool:echo work > work.txt; say:done", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NeedsForce));
    assert!(worktree.join("work.txt").exists());
    // Its record said it was being archived, and is put back.
    assert_eq!(client.workspace_list(false).unwrap()[0].status, WorkspaceStatus::Ready);
    client.session_prompt(&session.id, "say:the agent was left running by the refused archive", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let started = events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1);

    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "the agent's work"]);
    let archived = client.workspace_archive(&workspace.id, false).unwrap();
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    assert!(!worktree.exists());
    // Its files go after the reply, while the hub closes its worker, from where the worker moved them aside. Only the
    // lock that archives beside it share stays.
    let deleting = worktree.with_file_name(".deleting");
    let left = || std::fs::read_dir(&deleting).map(|entries| entries.flatten().map(|e| e.file_name()).collect());
    wait_until("its files to be deleted", || left().is_ok_and(|left: Vec<_>| left == [".lock"]));
    assert_eq!(client.workspace_list(false).unwrap().len(), 0);
    assert_eq!(client.workspace_list(true).unwrap().len(), 1);
    assert!(!git(&stack.repo(), &["branch", "--list", workspace.branch.as_str()]).is_empty());

    // Archiving it again can't work, and force wouldn't change that.
    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));

    // An archived workspace takes no prompts, and its history stays readable.
    let err = client.session_prompt(&session.id, "say:no", None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::Conflict));
    let before = read_all(&client, &session.id);
    assert!(before.len() > 3);

    let restored = client.workspace_restore(&workspace.id).unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert_eq!(std::fs::read_to_string(worktree.join("work.txt")).unwrap(), "work\n");
    client.session_prompt(&session.id, "recall", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 3);
    assert!(assistant_text(&events[before.len()..]).contains("tool:echo work"));
}

#[test]
fn archiving_keeps_a_branch_the_agent_renamed_and_restoring_finds_it() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:git branch -m fix-the-login-bug; say:renamed", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let archived = client.workspace_archive(&workspace.id, false).unwrap();
    assert_eq!(archived.branch.as_str(), "fix-the-login-bug");
    let restored = client.workspace_restore(&workspace.id).unwrap();
    let worktree = PathBuf::from(restored.path.unwrap());
    assert_eq!(git(&worktree, &["branch", "--show-current"]), "fix-the-login-bug");
}

#[test]
fn a_forced_archive_stops_a_running_agent() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut agent_pid = None;
    wait_until("the tool to start", || {
        let events = read_all(&client, &session.id);
        agent_pid = events.iter().find_map(|e| match &e.body {
            EventBody::AgentStarted { pid, .. } => Some(*pid),
            _ => None,
        });
        agent_pid.is_some() && events.iter().any(|e| matches!(e.body, EventBody::Assistant { .. }))
    });
    std::fs::write(PathBuf::from(workspace.path.clone().unwrap()).join("dirty.txt"), "x").unwrap();
    client.workspace_archive(&workspace.id, true).unwrap();
    assert!(!is_alive(agent_pid.unwrap()));
    assert!(!PathBuf::from(workspace.path.unwrap()).exists());
    let session = client.session_get(&session.id).unwrap();
    assert_eq!(session.status, SessionStatus::Interrupted);
}

#[test]
fn status_and_diff_come_from_the_worker() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    client.session_prompt(&session.id, "tool:echo changed >> a.txt && echo new > new.txt", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let status = client.workspace_status(&workspace.id).unwrap();
    assert_eq!(status.branch, Some(workspace.branch));
    let file = |path: &str| status.files.iter().find(|f| f.path == path).unwrap().status.clone();
    assert_eq!(file("a.txt"), ".M");
    assert_eq!(file("new.txt"), "??");

    let diff = client.workspace_diff(&workspace.id).unwrap();
    assert!(diff.patch.contains("+changed"), "{}", diff.patch);
    assert!(diff.patch.contains("+new"), "{}", diff.patch);
}

/// The worker lists and reads them, as they are and as the diff's base has them, whole or a page at a time, and its
/// own tests cover what it leaves out and refuses.
#[test]
fn a_client_browses_the_worktree_s_files_through_the_hub() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    let script = "tool:mkdir -p src build/out && echo 'fn main() {}' > src/main.rs && echo built > build/out/app.txt \
        && echo build/ >> .gitignore && echo changed > a.txt";
    client.session_prompt(&session.id, script, None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let base = client.workspace_diff(&workspace.id).unwrap().base_commit.unwrap();

    let files = client.workspace_files(&workspace.id).unwrap();
    let folder = client.workspace_folder_read(&workspace.id, "build/out").unwrap();
    let read = client.workspace_file_read(&workspace.id, "src/main.rs").unwrap();
    let before = client.workspace_file_read_at(&workspace.id, &base, "a.txt").unwrap();
    let FileOpened::Text(opened) = client.workspace_file_open_at(&workspace.id, &base, "a.txt").unwrap() else {
        panic!("a.txt isn't text");
    };
    let pages = client.workspace_file_pages(&workspace.id, &base, "a.txt", opened.page_lines, &[0]).unwrap();

    assert!(files.paths.iter().any(|path| path == "a.txt"), "a tracked file: {:?}", files.paths);
    assert!(files.paths.iter().any(|path| path == "src/main.rs"), "a new one: {:?}", files.paths);
    assert_eq!(files.folders, [".context", "build"], "ignored ones, by their folders, the agent's tmp folder's too");
    assert_eq!(folder.paths, ["build/out/app.txt"]);
    assert_eq!(read, FileContents::Text { text: "fn main() {}\n".to_string() });
    assert_eq!(before, FileContents::Text { text: "one\n".to_string() }, "as the diff's base has it");
    let one = vec![FilePage { index: 0, lines: vec![PageLine { text: "one".to_string(), cut: None }] }];
    assert_eq!((opened.lines, &opened.pages), (1, &one), "opened to be read in pages, and small enough to come whole");
    assert_eq!(pages.pages, one, "and read a page at a time");
}

#[test]
fn comments_are_kept_by_the_hub_and_survive_a_restart() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let added = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Rename this", None).unwrap();
    assert!(client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "  ", None).is_err(), "an empty comment");
    drop(client);
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.comment_list(&workspace.id).unwrap(), vec![added.clone()]);
    client.comment_delete(&added.id).unwrap();
    assert_eq!(client.comment_list(&workspace.id).unwrap(), vec![]);
}

#[test]
fn a_comment_is_edited_in_place() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let added = client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Rename this", None).unwrap();

    let edited = client.comment_edit(&added.id, "Rename this to `count`").unwrap();

    assert_eq!((&edited.id, edited.line, edited.text.as_str()), (&added.id, 1, "Rename this to `count`"));
    assert!(edited.rev > added.rev, "so clients can tell which copy is newer");
    assert_eq!(client.comment_list(&workspace.id).unwrap(), vec![edited.clone()]);
    let empty = client.comment_edit(&added.id, "  ").unwrap_err();
    assert!(matches!(empty, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));
    client.comment_delete(&added.id).unwrap();
    let gone = client.comment_edit(&added.id, "Too late").unwrap_err();
    assert!(matches!(gone, ClientError::Api(e) if e.code == ErrorCode::NotFound));
    assert!(client.comment_list(&workspace.id).unwrap().is_empty(), "editing doesn't bring it back");
}

/// So a client that loses the reply can send it again without making a second one.
#[test]
fn creating_again_with_the_same_request_id_returns_what_the_first_made() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    let (for_workspace, for_session, for_comment) = (new_request_id(), new_request_id(), new_request_id());
    let repo = &workspace.repo_id;
    let comment = |id: &ClientRequestId| {
        client.comment_add(&workspace.id, "a.txt", DiffSide::New, 1, "Rename this", Some(id.clone()))
    };
    let session = |id: &ClientRequestId| {
        client.session_create(&workspace.id, AgentKind::Claude, SessionSettingsChange::default(), Some(id.clone()))
    };

    let made = client.workspace_create(repo, None, None, Some(for_workspace.clone())).unwrap();
    let again = client.workspace_create(repo, None, None, Some(for_workspace.clone())).unwrap();
    let chats = [session(&for_session).unwrap(), session(&for_session).unwrap()];
    let comments = [comment(&for_comment).unwrap(), comment(&for_comment).unwrap()];

    // The same records, as they are now: the workspace's setup may have moved on.
    assert_eq!(again.id, made.id);
    assert_eq!(chats[0].id, chats[1].id);
    assert_eq!(comments[0].id, comments[1].id);
    assert_eq!(client.workspace_list(false).unwrap().len(), 2);
    assert_eq!(client.session_list(&workspace.id).unwrap().len(), 2);
    assert_eq!(client.comment_list(&workspace.id).unwrap().len(), 1);

    // Sent again after things have changed, it still gets back what it made, rather than a refusal.
    client.workspace_archive(&workspace.id, true).unwrap();
    assert_eq!(session(&for_session).unwrap().id, chats[0].id);
}

#[test]
fn a_workspace_with_no_pull_request_reports_none_rather_than_failing() {
    let stack = Stack::start();
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    assert_eq!(client.workspace_pull_request(&workspace.id).unwrap(), None);
}

#[test]
fn workspace_names_are_checked_and_collisions_are_handled() {
    let stack = Stack::start();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();

    let err = client.workspace_create(&repo.id, Some("../escape".into()), None, None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::InvalidRequest));

    let first = client.workspace_create(&repo.id, Some("mine".into()), None, None).unwrap();
    let err = client.workspace_create(&repo.id, Some("mine".into()), None, None).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NameTaken));

    // A branch that exists outside the hub also blocks a chosen name, and so does one in its folder, and either leaves
    // nothing behind.
    git(&stack.repo(), &["branch", "taken"]);
    git(&stack.repo(), &["branch", "folder/fix"]);
    for name in ["taken", "folder"] {
        let err = client.workspace_create(&repo.id, Some(name.into()), None, None).unwrap_err();
        assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NameTaken), "{name}");
    }
    assert_eq!(client.workspace_list(true).unwrap().len(), 1);
    assert_eq!(client.workspace_list(true).unwrap()[0].id, first.id);

    let generated = client.workspace_create(&repo.id, None, None, None).unwrap();
    assert!(hub::workspace_names().contains(&generated.name.as_str()), "{}", generated.name);
}

/// A name git has a branch by, or a folder of branches, is refused before anything is made, so the workspace takes
/// another name in place, and a client following the feed sees one workspace on its way throughout, rather than one
/// going and another coming. That's the same station with a number, which the worker's list of every name git has,
/// whatever their case, shows is free, though here git has every station. Its instance is made again under it, since a
/// provider's own tools call it by the name it was made with, and recorded as made, like any other.
#[test]
fn a_workspace_whose_name_git_has_takes_its_station_with_a_number_keeping_its_id() {
    let mut stack = Stack::start();
    stack.make_through_an_adapter();
    let client = stack.client();
    let feed = client.feed_subscribe().unwrap();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    next_notice(&feed, "the repo");
    for name in hub::workspace_names() {
        git(&stack.repo(), &["branch", &format!("{}/fix", name.to_uppercase())]);
    }

    let made = client.workspace_create(&repo.id, None, None, None).unwrap();

    let HubNotice::Workspace(first) = next_notice(&feed, "the workspace on its way") else { panic!() };
    let HubNotice::Workspace(renamed) = next_notice(&feed, "its next name") else { panic!() };
    assert_eq!((renamed.id.as_str(), renamed.status), (first.id.as_str(), WorkspaceStatus::Creating));
    assert_eq!((made.id, made.status), (first.id, WorkspaceStatus::Ready));
    assert_eq!(made.name, renamed.name);
    assert_eq!(made.name, format!("{}-2", first.name));
    assert_eq!(made.branch.as_str(), format!("{}/start", made.name));
    let made_as: Vec<String> = [&first.name, &made.name].iter().map(|name| format!("roundhouse-{name}")).collect();
    assert_eq!(stack.instances_made(), made_as);
    stack.kill_hub();
    let paths = stack.paths();
    let store = hub::store::Store::open(&paths.database, &paths.backups_dir).unwrap();
    let recorded: Vec<_> = store.recorded_instances().unwrap().into_iter().map(|i| (i.id, i.stage)).collect();
    assert_eq!(recorded, [(made.instance_id, hub::store::InstanceStage::Ready)]);
}

#[test]
fn an_archive_refused_for_work_it_would_lose_changes_nothing() {
    let stack = Stack::start();
    let marker = stack.path("archived-marker");
    commit_settings(&stack, &format!("[scripts]\narchive = \"touch {}\"\n", marker.display()));
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    wait_for_setup(&client, &workspace.id);
    client.session_prompt(&session.id, "say:hello", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);
    let worktree = PathBuf::from(workspace.path.clone().unwrap());

    // Commits on no branch.
    git(&worktree, &["checkout", "-q", "--detach"]);
    git(&worktree, &["commit", "-q", "--allow-empty", "-m", "on no branch"]);
    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NeedsForce));
    assert!(!marker.exists(), "the archive script mustn't run");

    // Uncommitted changes.
    std::fs::write(worktree.join("work.txt"), "work\n").unwrap();
    let err = client.workspace_archive(&workspace.id, false).unwrap_err();
    assert!(matches!(err, ClientError::Api(e) if e.code == ErrorCode::NeedsForce));
    assert!(!marker.exists(), "the archive script mustn't run");

    client.session_prompt(&session.id, "say:still here", None).unwrap();
    let events = wait_for_turn_end(&client, &session.id, 2);
    let started = events.iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the agent mustn't have been stopped");

    client.workspace_archive(&workspace.id, true).unwrap();
    assert!(marker.exists());
    assert!(!worktree.exists());
}
