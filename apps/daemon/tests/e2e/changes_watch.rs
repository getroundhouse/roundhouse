//! A workspace's worker watching its worktree while a client follows the workspace, so the summary of its changes,
//! which that client holds, and the lines every client that follows the hub holds, are current within a moment of a
//! file changing, whoever changed it, and not only as each turn ends. The hub's tests cover when it tells a worker to
//! watch, and the worker's which writes wake a summary.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::client::HubFrame;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::Follow;
use protocol::model::{ChangeSummary, ChangedFile, LineCounts, Observed};

use crate::support::{RawConnection, Stack, changes_in, following, wait_for_turn_end, workspace};

/// A workspace whose worker a turn has started, with an untracked `.gitignore` that ignores `target/`, and a connection
/// that follows the workspace, once the summary it holds is current, and that summary.
fn followed(stack: &Stack) -> (PathBuf, RawConnection, ChangeSummary) {
    let client = stack.client();
    let (made, session) = stack.workspace(&client);
    let worktree = PathBuf::from(workspace(&client, &made.id).path.unwrap());
    std::fs::write(worktree.join(".gitignore"), "target/\n").unwrap();
    client.session_prompt(&session.id, "tool:true", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    let mut connection = following(stack, vec![Follow::Workspace { id: made.id }]);
    let listed =
        connection.sent.iter().any(|frame| matches!(frame, HubFrame::Listed { listed: Listed::Workspace(_), .. }));
    assert!(listed, "where the workspace stands came before the reply: {:#?}", connection.sent);
    let summary = next_current(&mut connection, "the summary to be current", |_| true);
    (worktree, connection, summary)
}

/// The summary in the next notice on `connection` that says it's current and holds `wanted`.
fn next_current(connection: &mut RawConnection, what: &str, wanted: impl Fn(&ChangeSummary) -> bool) -> ChangeSummary {
    let before = connection.sent.len();
    let current = |frame: &HubFrame| match changes_in(frame) {
        Some(Observed::Current { value }) if wanted(value) => Some(value.clone()),
        _ => None,
    };
    connection.until(what, |sent| sent[before..].iter().any(|frame| current(frame).is_some()));
    connection.sent[before..].iter().find_map(current).unwrap()
}

fn file<'a>(summary: &'a ChangeSummary, path: &str) -> Option<&'a ChangedFile> {
    summary.files.iter().find(|file| file.path == path)
}

fn write(worktree: &Path, path: &str, text: &str) {
    let file = worktree.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// A client that follows a workspace holds its summary as current, and an edit made outside any turn, as a person's
/// editor makes, brings it a notice whose summary has the file's new version and lines, though it asked for nothing.
#[test]
fn an_edit_outside_any_turn_reaches_a_client_that_follows_the_workspace() {
    let stack = Stack::start();
    let (worktree, mut connection, first) = followed(&stack);

    write(&worktree, "edited.txt", "one\ntwo\nthree\n");
    let made = next_current(&mut connection, "the new file", |summary| file(summary, "edited.txt").is_some());
    write(&worktree, "edited.txt", "one\ntwo\nthree\nfour\n");
    let edited = next_current(&mut connection, "the edit", |summary| {
        file(summary, "edited.txt").is_some_and(|edited| edited.lines == Some(LineCounts { added: 4, removed: 0 }))
    });

    assert!(file(&first, "edited.txt").is_none());
    let (made, edited) = (file(&made, "edited.txt").unwrap(), file(&edited, "edited.txt").unwrap());
    assert!(made.version.is_some() && edited.version.is_some(), "{made:?}, {edited:?}");
    assert_ne!(made.version, edited.version, "the edit gave it another version");
}

/// A build writing into a folder git ignores brings a client that follows the workspace no notice, and an edit made
/// after it still does. That the build wakes no summary at all is the worker's own test,
/// `an_ignored_folder_wakes_no_summary`.
#[test]
fn a_build_into_an_ignored_folder_brings_no_summary() {
    let stack = Stack::start();
    let (worktree, mut connection, _) = followed(&stack);

    for n in 0..1_000 {
        write(&worktree, &format!("target/debug/deps/{}/{n}.o", n % 10), "built\n");
    }
    // Ten times the quiet a summary waits for, to give one the build woke time to arrive.
    let after_the_build: Vec<HubFrame> = connection.within(Duration::from_secs(1)).to_vec();
    write(&worktree, "edited.txt", "one\n");
    let edited = next_current(&mut connection, "the edit", |summary| file(summary, "edited.txt").is_some());

    assert!(!after_the_build.iter().any(|frame| changes_in(frame).is_some()), "{after_the_build:#?}");
    assert!(edited.files.iter().all(|file| !file.path.starts_with("target/")), "{edited:?}");
}
