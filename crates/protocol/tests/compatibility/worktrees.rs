//! What a hub and its worker say of a workspace's other worktrees, spelled out, since a worker can outlive the hub
//! that started it, and the next hub can be of another build.

use protocol::frame;
use protocol::model::{
    AskKey, BranchName, Branches, CHILDREN, ChildStatus, ChildWorktree, ChildWorktreeId, CommitId, GitStatus, Head,
    MadeBy, Rev, WorkName,
};
use protocol::worker::{
    AddChildWorktree, Ask as _, AskReply, ChildBranch, ChildWorktreeAdded, CreateWorktree, ListWorktrees, ListedChild,
    NewWorktreeOn, OwnWorktree, RemoveWorktree, Request as _, WatchChildBranches, WorkerAsk, WorkerFrame,
    WorkerRequest, WorkerResponse, WorktreeCreated, WorktreeRemoved, Worktrees, WouldLose,
};

use super::keeps_its_form;
use super::worker_calls::{answered, asked};

const NOTHING_CHANGED: &str =
    r#"{"branch":null,"head":null,"ahead":0,"behind":0,"files":[],"unreferenced_commits":false}"#;

fn on(branch: &str) -> Branches {
    Branches { head: Head::Branch { name: BranchName::from(branch) }, had: vec![] }
}

/// One a subagent made, removed since, and how it's written.
fn removed() -> (ChildWorktree, &'static str) {
    let child = ChildWorktree {
        id: ChildWorktreeId::from("c1"),
        workspace_id: "w".into(),
        path: "/c/holly/x".into(),
        branch: BranchName::from("holly/x"),
        session_id: "s".into(),
        made_by: MadeBy::Subagent { call: "t".into() },
        created_at_ms: 5,
        status: ChildStatus::Removed { removed_at_ms: 6 },
        branches: None,
        rev: Rev::default(),
    };
    let wire = r#"{"id":"c1","workspace_id":"w","path":"/c/holly/x","branch":"holly/x","session_id":"s","made_by":{"kind":"subagent","call":"t"},"created_at_ms":5,"status":{"kind":"removed","removed_at_ms":6},"rev":0}"#;
    (child, wire)
}

/// One the chat's own agent made, which is there with its branches read, and how it's written.
fn there() -> (ChildWorktree, &'static str) {
    let child = ChildWorktree {
        made_by: MadeBy::Agent,
        status: ChildStatus::Ready,
        branches: Some(on("holly/x")),
        rev: Rev::new(7),
        ..removed().0
    };
    let wire = r#"{"id":"c1","workspace_id":"w","path":"/c/holly/x","branch":"holly/x","session_id":"s","made_by":{"kind":"agent"},"created_at_ms":5,"status":{"kind":"ready"},"branches":{"head":{"kind":"branch","name":"holly/x"},"had":[]},"rev":7}"#;
    (child, wire)
}

/// What a worker asks of the workspace's other worktrees, with what it may leave out left out and then said. A name
/// that isn't one doesn't read as an ask, so the hub refuses it as one it can't read.
#[test]
fn the_worktree_asks_keep_their_form() {
    let name = WorkName::try_from("try-the-parser".to_string()).unwrap();
    let key = AskKey::from("k");
    let new = |base: Option<&str>| {
        let on = NewWorktreeOn::NewBranch { name: name.clone(), base: base.map(str::to_string) };
        CreateWorktree { on, key: key.clone() }.into_ask()
    };
    let existing = NewWorktreeOn::Existing { branch: BranchName::from("holly/x") };
    let remove = |force| RemoveWorktree { path: "/c".into(), force }.into_ask();

    keeps_its_form(
        &new(None),
        r#"{"method":"create_worktree","on":{"kind":"new_branch","name":"try-the-parser"},"key":"k"}"#,
    );
    keeps_its_form(
        &new(Some("origin/main")),
        r#"{"method":"create_worktree","on":{"kind":"new_branch","name":"try-the-parser","base":"origin/main"},"key":"k"}"#,
    );
    keeps_its_form(
        &CreateWorktree { on: existing, key: key.clone() }.into_ask(),
        r#"{"method":"create_worktree","on":{"kind":"existing","branch":"holly/x"},"key":"k"}"#,
    );
    keeps_its_form(&remove(false), r#"{"method":"remove_worktree","path":"/c"}"#);
    keeps_its_form(&remove(true), r#"{"method":"remove_worktree","path":"/c","force":true}"#);
    keeps_its_form(&ListWorktrees.into_ask(), r#"{"method":"list_worktrees"}"#);
    let newer: WorkerAsk = serde_json::from_str(r#"{"method":"list_worktrees","newer":1}"#).unwrap();
    assert_eq!(newer, ListWorktrees.into_ask(), "an ask with no fields reads with one a newer worker added");
    let unnamed = r#"{"method":"create_worktree","on":{"kind":"new_branch","name":"Try The Parser"},"key":"k"}"#;
    assert!(serde_json::from_str::<WorkerAsk>(unnamed).is_err());
}

/// The hub's answers about the workspace's other worktrees, with what each may leave out left out and then said.
#[test]
fn the_worktree_answers_keep_their_form() {
    let ((gone, gone_wire), (there, there_wire)) = (removed(), there());
    let created = |copied: &[&str], warnings: &[&str]| {
        AskReply::WorktreeCreated(WorktreeCreated {
            worktree: there.clone(),
            commit: CommitId::from("abc"),
            copied: copied.iter().map(ToString::to_string).collect(),
            warnings: warnings.iter().map(ToString::to_string).collect(),
        })
    };
    let listed = |status: Option<GitStatus>| {
        AskReply::Worktrees(Worktrees {
            own: OwnWorktree { path: "/w/holly".into(), status: status.clone() },
            children: vec![ListedChild { worktree: there.clone(), status }],
        })
    };

    keeps_its_form(
        &created(&[], &[]),
        &format!(r#"{{"result":"worktree_created","worktree":{there_wire},"commit":"abc"}}"#),
    );
    keeps_its_form(
        &created(&[".env"], &["no .env.local"]),
        &format!(
            r#"{{"result":"worktree_created","worktree":{there_wire},"commit":"abc","copied":[".env"],"warnings":["no .env.local"]}}"#
        ),
    );
    keeps_its_form(
        &AskReply::WorktreeRemoved(WorktreeRemoved { worktree: gone.clone(), last: None }),
        &format!(r#"{{"result":"worktree_removed","worktree":{gone_wire}}}"#),
    );
    keeps_its_form(
        &AskReply::WorktreeRemoved(WorktreeRemoved { worktree: gone, last: Some(GitStatus::default()) }),
        &format!(r#"{{"result":"worktree_removed","worktree":{gone_wire},"last":{NOTHING_CHANGED}}}"#),
    );
    keeps_its_form(
        &AskReply::WouldLose(WouldLose { status: GitStatus::default() }),
        &format!(r#"{{"result":"would_lose","status":{NOTHING_CHANGED}}}"#),
    );
    keeps_its_form(
        &listed(None),
        &format!(r#"{{"result":"worktrees","own":{{"path":"/w/holly"}},"children":[{{"worktree":{there_wire}}}]}}"#),
    );
    keeps_its_form(
        &listed(Some(GitStatus::default())),
        &format!(
            r#"{{"result":"worktrees","own":{{"path":"/w/holly","status":{NOTHING_CHANGED}}},"children":[{{"worktree":{there_wire},"status":{NOTHING_CHANGED}}}]}}"#
        ),
    );
}

/// The rest of the ways a record says who made it and where it is in its life, and one from a newer hub, whose kinds
/// this build doesn't know, still reads.
#[test]
fn a_record_keeps_the_form_of_each_kind_and_reads_kinds_it_doesnt_know() {
    let being_made = ChildWorktree { made_by: MadeBy::Undetermined, status: ChildStatus::Creating, ..removed().0 };
    keeps_its_form(
        &being_made,
        r#"{"id":"c1","workspace_id":"w","path":"/c/holly/x","branch":"holly/x","session_id":"s","made_by":{"kind":"undetermined"},"created_at_ms":5,"status":{"kind":"creating"},"rev":0}"#,
    );

    let newer = r#"{"id":"c1","workspace_id":"w","path":"/p","branch":"b","session_id":"s","made_by":{"kind":"a_hook","name":"h"},"created_at_ms":5,"status":{"kind":"moved","to":"/x","at":[1]}}"#;
    let newer: ChildWorktree = serde_json::from_str(newer).unwrap();
    assert_eq!((newer.made_by, newer.status, newer.rev), (MadeBy::Unknown, ChildStatus::Unknown, Rev::default()));
}

/// The folder's name is how a hub and a worker of other builds agree where these go. A worker that doesn't know what a
/// worktree is to check out refuses the request, and a hub that doesn't refuses the ask, rather than make it some other
/// way.
#[test]
fn the_folder_keeps_its_name_and_a_branch_of_a_kind_this_build_doesnt_know_isnt_read() {
    assert_eq!(CHILDREN, ".children");

    let newer = br#"{"id":7,"request":{"method":"add_child_worktree","repo_path":"/r","own":"/w/holly","path":"/w/.children/holly/x","on":{"kind":"detached","at":"abc"}}}"#;
    let call = frame::read_call::<WorkerRequest>(newer).unwrap();
    assert!((call.id, call.request.is_err()) == (7, true), "{call:?}");
    let ask = r#"{"method":"create_worktree","on":{"kind":"detached"},"key":"k"}"#;
    assert!(serde_json::from_str::<WorkerAsk>(ask).is_err());
}

/// What the hub asks a worker to make and watch of the workspace's other worktrees, and what comes back.
#[test]
fn the_child_worktree_requests_and_frames_keep_their_form() {
    let add = |on| AddChildWorktree {
        repo_path: "/r".into(),
        own: "/w/holly".into(),
        path: "/w/.children/holly/x".into(),
        on,
    };
    asked(
        add(ChildBranch::New { branch: BranchName::from("holly/x"), base: "HEAD".into() }).into_request(),
        r#"{"method":"add_child_worktree","repo_path":"/r","own":"/w/holly","path":"/w/.children/holly/x","on":{"kind":"new","branch":"holly/x","base":"HEAD"}}"#,
    );
    asked(
        add(ChildBranch::Existing { branch: BranchName::from("holly/x") }).into_request(),
        r#"{"method":"add_child_worktree","repo_path":"/r","own":"/w/holly","path":"/w/.children/holly/x","on":{"kind":"existing","branch":"holly/x"}}"#,
    );
    asked(
        WatchChildBranches { path: "/c/holly/x".into() }.into_request(),
        r#"{"method":"watch_child_branches","path":"/c/holly/x"}"#,
    );

    let added = |copied: &[&str], warnings: &[&str], branches| {
        WorkerResponse::ChildWorktreeAdded(ChildWorktreeAdded {
            commit: CommitId::from("abc"),
            copied: copied.iter().map(ToString::to_string).collect(),
            warnings: warnings.iter().map(ToString::to_string).collect(),
            branches,
        })
    };
    answered(added(&[], &[], None), r#"{"result":"child_worktree_added","commit":"abc"}"#);
    answered(
        added(&[".env"], &["no .env.local"], Some(on("holly/x"))),
        r#"{"result":"child_worktree_added","commit":"abc","copied":[".env"],"warnings":["no .env.local"],"branches":{"head":{"kind":"branch","name":"holly/x"},"had":[]}}"#,
    );
    keeps_its_form(
        &WorkerFrame::ChildBranches { path: "/c/holly/x".into(), branches: on("holly/x") },
        r#"{"frame":"child_branches","path":"/c/holly/x","branches":{"head":{"kind":"branch","name":"holly/x"},"had":[]}}"#,
    );
}
