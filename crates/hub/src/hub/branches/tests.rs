use protocol::model::{Branch, BranchName, CommitId, RemoteName};

use super::*;
use crate::hub::tests::{hub_with_worktree, reading, says};

/// Branches with HEAD on `name`, each with its newest commit, and what the worktree knows of its upstream's, if it has
/// one, which has the same name.
fn on(name: &str, had: &[(&str, &str, Option<&str>)]) -> Branches {
    let had = had
        .iter()
        .map(|&(name, tip, pushed)| Branch {
            name: BranchName::from(name),
            tip: CommitId::from(tip),
            upstream: pushed.map(|tip| Upstream {
                remote: RemoteName::from("origin"),
                name: BranchName::from(name),
                tip: Some(CommitId::from(tip)),
            }),
        })
        .collect();
    Branches { head: Head::Branch { name: BranchName::from(name) }, had }
}

#[test]
fn checking_out_another_branch_switches_and_detaching_doesnt() {
    let before = on("aspen", &[("aspen", "1", Some("1")), ("header", "2", None)]);
    let detached = Branches { head: Head::Detached { commit: CommitId::from("1") }, ..before.clone() };

    let switched = moved(&before, &on("header", &[("aspen", "1", Some("1")), ("header", "2", None)]));
    let detaching = moved(&before, &detached);
    let back = moved(&detached, &before);

    assert_eq!(switched, Some(Moved::Switched));
    assert_eq!(detaching, None, "a detached HEAD has no pull request to read");
    assert_eq!(back, Some(Moved::Switched), "which reads only if what's known is another branch's");
}

#[test]
fn pushing_is_a_push_and_committing_isnt() {
    let before = on("aspen", &[("aspen", "1", Some("1")), ("header", "2", None)]);

    let committed = on("aspen", &[("aspen", "3", Some("1")), ("header", "2", None)]);
    let pushed = on("aspen", &[("aspen", "3", Some("3")), ("header", "2", None)]);
    let other_pushed = on("aspen", &[("aspen", "1", Some("1")), ("header", "2", Some("2"))]);

    assert_eq!(moved(&before, &committed), None, "GitHub has nothing new until it's pushed");
    assert_eq!(moved(&committed, &pushed), Some(Moved::Pushed));
    assert_eq!(moved(&before, &other_pushed), None, "only the branch checked out has its pull request read");
}

/// As after `git checkout main && git pull`, which moves the branch and what the worktree knows of its upstream to
/// the same commit.
#[test]
fn a_pull_isnt_a_push() {
    let before = on("main", &[("main", "1", Some("1"))]);

    let pulled = on("main", &[("main", "2", Some("2"))]);

    assert_eq!(moved(&before, &pulled), None);
}

#[test]
fn a_first_push_is_a_push() {
    let before = on("aspen", &[("aspen", "1", None)]);

    let pushed = on("aspen", &[("aspen", "1", Some("1"))]);

    assert_eq!(moved(&before, &pushed), Some(Moved::Pushed));
}

/// As for a branch made with `git switch -c next origin/main`, which tracks `origin/main`, when any agent in the
/// repository fetches it.
#[test]
fn a_fetch_that_moves_the_upstream_away_from_the_branch_isnt_a_push() {
    let tracking = |local: &str, there: &str| {
        let mut branches = on("next", &[("next", local, Some(there))]);
        branches.had[0].upstream.as_mut().unwrap().name = BranchName::from("main");
        branches
    };

    let fetched = moved(&tracking("1", "1"), &tracking("1", "2"));
    let ahead_and_fetched = moved(&tracking("3", "1"), &tracking("3", "2"));

    assert_eq!(fetched, None);
    assert_eq!(ahead_and_fetched, None);
}

/// A worker that's been replaced may still have frames on the way, which say less than the new one's.
#[tokio::test]
async fn only_the_worker_the_workspace_has_now_is_heard() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let says = says(None);
    let replaced = reading(&hub, &says);
    let current = reading(&hub, &says);
    let id = WorkspaceId::from("w");

    let frame = |branch: &str| WorkerFrame::Branches { branches: on(branch, &[]) };
    hub.branches_frame(&id, &replaced, frame("aspen")).await;
    let from_replaced = hub.workspace(&id).await.unwrap().branches;
    hub.branches_frame(&id, &current, frame("header")).await;
    let from_current = hub.workspace(&id).await.unwrap().branches;

    assert_eq!(from_replaced, None);
    assert_eq!(from_current, Some(on("header", &[])));
}
