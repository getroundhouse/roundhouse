//! Each workspace's pull request, which the hub keeps the link to on the workspace's record, read from a stand-in for
//! GitHub by the branch there that the workspace's branch is pushed to.

#![cfg(test)]

use std::path::Path;

use client_core::{Client, ClientError};
use hub::GithubPullRequestState::{Merged, Open};
use protocol::client::ErrorCode;
use protocol::model::{BranchName, Head, PrState, PullRequestLinked, WorkspaceId};

use crate::support::github::{StandInGithub, push, pushed_workspace, workspace_on_github};
use crate::support::{self, is_alive, started_pid, wait_for_turn_end, wait_until};

/// The number of the pull request the workspace's record links to, as every client gets it.
fn recorded(client: &Client, id: &WorkspaceId) -> Option<u64> {
    support::workspace(client, id).pull_request_linked().map(|link| link.number.get())
}

#[test]
fn one_client_asking_records_the_pull_request_for_every_client_and_for_after_a_restart() {
    let github = StandInGithub::start();
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, _) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());

    let asked = client.workspace_pull_request(&workspace.id).unwrap().unwrap();
    github.fail(true);
    stack.restart_hub();
    let client = stack.client();

    let url = "https://github.com/example/project/pull/7";
    let said = asked.pull_request.said();
    assert_eq!((asked.pull_request.number.get(), said.state, said.about.url.as_str()), (7, PrState::Open, url));
    // GitHub is out of reach, so this is what the last hub found.
    assert_eq!(recorded(&client, &workspace.id), Some(7), "kept through the restart");
}

/// Which pull request is a workspace's goes by the branch on GitHub its branch is pushed to, as the hub's own tests of
/// `own::judging` say case by case. This walks them on one branch, through a real worker's reports of it, for what only
/// the whole read shows: what GitHub is asked, and what the hub records.
#[test]
fn which_pull_request_is_the_workspaces_goes_by_the_branch_its_pushed_to() {
    let github = StandInGithub::start();
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, _) = workspace_on_github(&stack, &client);
    let branch = workspace.branch.as_str();
    let read = || client.workspace_pull_request(&workspace.id).unwrap();
    let found = || support::workspace(&client, &workspace.id).pull_request_link;
    let looked_and_found_none = Some(PullRequestLinked { link: None });

    // Pushed nowhere, so there's none, which needs nothing of GitHub to say.
    github.say_from(7, Merged, branch);
    let unpushed = (read(), github.queries());
    // Pushed, to a branch whose only pull request merged from an earlier branch of the same name, whose newest commit
    // was never here.
    push(&client, &workspace);
    let earlier = (read(), found());
    // Opened from it, with a commit someone else pushed, which the worktree hasn't fetched.
    github.say_from(8, Open, branch);
    let opened = read();
    // GitHub out of reach says nothing about it, so what was known is kept.
    github.fail(true);
    let out_of_reach = read();
    github.fail(false);
    // Merged since, and a newer one opened from the branch.
    github.say_from(8, Merged, branch);
    github.say_from(9, Open, branch);
    let newer = read();
    // Gone from the branch, as when its head is changed to another, so the branch has none of its own.
    github.say_from(9, Open, "another-branch");
    let gone = (read(), found());

    assert_eq!(unpushed, (None, 0), "pushed nowhere");
    assert_eq!(earlier, (None, looked_and_found_none.clone()), "an earlier branch's: looked, and found none");
    let opened = opened.expect("opened from the branch");
    assert_eq!(
        (opened.pull_request.number.get(), opened.local.ahead, opened.local.behind),
        (8, 0, 1),
        "with a commit to pull"
    );
    assert_eq!(out_of_reach.map(|pr| pr.pull_request.number.get()), Some(8), "kept while GitHub is out of reach");
    assert_eq!(
        newer.map(|pr| (pr.pull_request.number.get(), pr.pull_request.said().state)),
        Some((9, PrState::Open)),
        "the newer one"
    );
    assert_eq!(gone, (None, looked_and_found_none), "gone from the branch: looked, and found none");
}

/// Out of reach, it may well be there, so with nothing known the read fails, and it stays unread.
#[test]
fn with_nothing_known_a_read_github_cant_answer_fails() {
    let github = StandInGithub::start();
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, _) = workspace_on_github(&stack, &client);
    // Before the push, so the read it brings finds nothing either.
    github.fail(true);
    push(&client, &workspace);

    let failed = client.workspace_pull_request(&workspace.id);

    assert!(matches!(&failed, Err(ClientError::Api(e)) if e.code == ErrorCode::Worker), "{failed:?}");
    assert_eq!(support::workspace(&client, &workspace.id).pull_request_link, None, "still not looked at");
}

/// As after an update, when nobody has opened any workspace yet. Both are read as the machine's own `gh`, together.
#[test]
fn a_starting_hub_reads_every_workspaces_pull_request_without_being_asked() {
    let github = StandInGithub::start();
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (first, _) = pushed_workspace(&stack, &client);
    let (second, _) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, first.branch.as_str());
    github.say_from(8, Open, second.branch.as_str());

    stack.restart_hub();
    let client = stack.client();

    wait_until("both to be read", || {
        recorded(&client, &first.id) == Some(7) && recorded(&client, &second.id) == Some(8)
    });
}

/// An agent may have opened one in its turn, so its end reads it, whether or not anyone is looking at the workspace.
/// It was just made, so nothing else reads it this soon.
#[test]
fn the_end_of_a_turn_reads_the_pull_request() {
    let github = StandInGithub::start();
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, session) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());

    client.session_prompt(&session.id, "say:opened one", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    wait_until("the pull request on its record", || recorded(&client, &workspace.id) == Some(7));
}

/// A pull request is its branch's, so a merged one is the workspace's only until the worktree checks out another
/// branch, as an agent does to start on its next piece of work. The worker says so as it happens, and the hub reads it
/// again then, rather than once the turn ends or someone shows the workspace, though a merged one isn't read otherwise.
#[test]
fn checking_out_another_branch_reads_the_pull_request_again_though_it_was_merged() {
    let github = StandInGithub::start();
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, _) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());
    client.workspace_pull_request(&workspace.id).unwrap();
    github.say_from(7, Merged, workspace.branch.as_str());
    let merged = client.workspace_pull_request(&workspace.id).unwrap().map(|pr| pr.pull_request.said().state);
    let head = |client: &Client| support::workspace(client, &workspace.id).branches.map(|branches| branches.head);
    let started_on = Head::Branch { name: workspace.branch.clone() };
    assert_eq!((head(&client), merged), (Some(started_on), Some(PrState::Merged)), "known before it moves");

    support::git(Path::new(&workspace.path.unwrap()), &["switch", "-q", "-c", "next-thing"]);

    wait_until("the pull request to be read again", || recorded(&client, &workspace.id).is_none());
    let found_on = support::workspace(&client, &workspace.id).pull_request_branch;
    assert_eq!(head(&client), Some(Head::Branch { name: BranchName::from("next-thing") }));
    assert_eq!(found_on, Some(BranchName::from("next-thing")), "found on the branch it's on now");
}

/// A workspace nobody has open has no worker once the hub starts again. The hub needs one only for its machine's
/// `gh`'s token, and one started for that doesn't stay, so two hundred workspaces don't keep two hundred running. The
/// next prompt starts one as usual.
#[test]
fn a_worker_started_only_for_a_read_is_stopped_after() {
    let github = StandInGithub::start();
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, session) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());

    stack.restart_hub();
    let client = stack.client();
    wait_until("it to be read", || recorded(&client, &workspace.id) == Some(7));

    wait_until("its worker to stop", || client.hub_status().unwrap().workers == 0);
    client.session_prompt(&session.id, "say:still here", None).unwrap();
    assert!(support::assistant_text(&wait_for_turn_end(&client, &session.id, 1)).ends_with("still here"));
}

/// A prompt sent while a read nobody asked for has the worker it started takes the worker over, so it isn't stopped
/// under the prompt's agent once the read is done.
#[test]
fn a_worker_started_only_for_a_read_stays_once_a_prompt_wants_it() {
    let github = StandInGithub::start();
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, session) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());
    let asked = github.queries();
    github.hold();

    stack.restart_hub();
    let client = stack.client();
    wait_until("it to be read again", || github.queries() > asked);
    client.session_prompt(&session.id, "hang", None).unwrap();
    let mut agent = None;
    wait_until("the prompt's agent to start", || {
        agent = started_pid(&support::read_all(&client, &session.id));
        agent.is_some()
    });
    github.release();
    wait_until("it to be read", || recorded(&client, &workspace.id) == Some(7));

    assert!(is_alive(agent.unwrap()), "the prompt's agent went with the read's worker");
}

/// A read nobody asked for leaves the last fifth of GitHub's limit to agents and to reads someone is waiting on, so
/// once GitHub says that's all that's left, it stops, and what's left is there for a read someone waits on.
#[test]
fn reads_nobody_asked_for_leave_the_last_of_githubs_limit_to_the_rest() {
    let github = StandInGithub::start();
    let (mut stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, _) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());
    github.leave(900);
    let asked = github.queries();

    stack.restart_hub();
    let client = stack.client();
    wait_until("GitHub to be asked", || github.queries() > asked);
    wait_until("the worker started for the token to stop", || client.hub_status().unwrap().workers == 0);

    assert_eq!(github.queries(), asked + 1, "it asked only which pull requests the branch has");
    assert_eq!(recorded(&client, &workspace.id), None, "it wasn't read");
    let read = client.workspace_pull_request(&workspace.id).unwrap();
    assert_eq!(
        read.map(|pr| pr.pull_request.number.get()),
        Some(7),
        "what's left is there for a read someone waits on"
    );
}

/// GitHub would refuse it anyway, and each refused call is one more against the agents sharing the limit. With
/// nothing known, that fails, rather than saying there's none, which would offer to create one that may be there.
#[test]
fn with_githubs_limit_spent_a_read_says_what_is_known_without_asking_github() {
    let github = StandInGithub::start();
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, _) = workspace_on_github(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());
    // Before the push, so the read it brings finds nothing either.
    github.leave(0);
    push(&client, &workspace);
    // Which GitHub says leaves nothing, if the push's read hasn't.
    let _ = client.workspace_pull_request(&workspace.id);
    let asked = github.queries();

    let answer = client.workspace_pull_request(&workspace.id);

    assert!(matches!(&answer, Err(ClientError::Api(e)) if e.code == ErrorCode::Unavailable), "{answer:?}");
    assert_eq!(github.queries(), asked, "nothing asked GitHub for it");
    assert_eq!(support::workspace(&client, &workspace.id).pull_request_link, None, "it still hasn't been looked at");
}

/// Asked for again while a read is on its way, the next read is still needed, since the first began before the ask.
/// But a third, asked for while the second waits, has nothing to add once the second has read.
#[test]
fn a_read_asked_for_while_another_waits_shares_its_answer() {
    let github = StandInGithub::start();
    let (stack, _gh) = github.stack();
    let client = stack.client();
    let (workspace, _) = pushed_workspace(&stack, &client);
    github.say_from(7, Open, workspace.branch.as_str());
    let before = github.queries();
    client.workspace_pull_request(&workspace.id).unwrap();
    let (asked, a_read) = (github.queries(), github.queries() - before);
    github.hold();
    let socket = stack.socket();
    let ask = |id: WorkspaceId| {
        let socket = socket.clone();
        std::thread::spawn(move || Client::connect(&socket, "test").unwrap().workspace_pull_request(&id).unwrap())
    };
    let first = ask(workspace.id.clone());
    wait_until("the first to reach GitHub", || github.queries() == asked + 1);
    let (second, third) = (ask(workspace.id.clone()), ask(workspace.id.clone()));
    // Each waiting, and the status asked for here.
    wait_until("both to wait", || client.hub_status().unwrap().requests == 4);

    github.release();
    let answers: Vec<_> = [first, second, third].into_iter().map(|asked| asked.join().unwrap()).collect();

    assert!(answers.iter().all(|pr| pr.as_ref().map(|pr| pr.pull_request.number.get()) == Some(7)), "{answers:?}");
    assert_eq!(github.queries(), asked + 2 * a_read, "the third shared the second's read");
}
