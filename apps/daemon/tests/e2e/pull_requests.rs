//! Each workspace's pull request, which the hub keeps on the workspace's record, from a stand-in `gh`, since GitHub
//! can't be reached from a test.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use client_core::{Client, ClientError};
use protocol::client::ErrorCode;
use protocol::model::{PrState, PullRequestFound, WorkspaceId};

use crate::support::{self, Stack, install_executable, is_alive, started_pid, wait_for_turn_end, wait_until};

/// What `gh pr view` prints for an open pull request.
const OPEN: &str = r#"{"isDraft":false,"number":7,"state":"OPEN","title":"T","url":"https://example.com/7"}"#;
/// What it says for a branch with none, in its own words, as against failing.
const NONE: &str = "none";
/// What it says when it can't reach GitHub.
const OFFLINE: &str = "offline";
/// What it says before `gh auth login`.
const LOGGED_OUT: &str = "logged-out";

/// A `gh` at the front of the hub's `PATH`, which says what a test tells it to for `gh pr view` and `gh api rate_limit`,
/// that the pull request isn't in a merge queue, and fails anything else. It writes down each time it's run.
struct StandInGh {
    bin: PathBuf,
    says: PathBuf,
    rate_limit: PathBuf,
    ran: PathBuf,
    /// While this file is there, `gh pr view` waits.
    held: PathBuf,
}

impl StandInGh {
    /// Replaces the stack's hub with one that uses it. Until told otherwise, it says there's no pull request, and that
    /// nothing of GitHub's limit has been spent.
    fn start(stack: &mut Stack) -> Self {
        stack.kill_hub();
        let bin = stack.path("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = Self {
            bin,
            says: stack.path("gh-says"),
            rate_limit: stack.path("gh-rate-limit"),
            ran: stack.path("gh-ran"),
            held: stack.path("gh-held"),
        };
        let script = format!(
            r#"#!/bin/sh
echo "$1 $2" >> '{ran}'
[ "$1 $2" = 'api rate_limit' ] && exec cat '{rate_limit}'
[ "$1 $2" = 'api graphql' ] && exec echo '{{"data":{{"resource":{{"mergeQueueEntry":null}}}}}}'
[ "$1 $2" = 'pr view' ] || exit 1
while [ -f '{held}' ]; do sleep 0.02; done
case "$(cat '{says}')" in
  {NONE}) echo 'no pull requests found for branch "x"' >&2; exit 1 ;;
  {OFFLINE}) echo 'error connecting to api.github.com' >&2; exit 1 ;;
  {LOGGED_OUT}) echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 4 ;;
  *) exec cat '{says}' ;;
esac
"#,
            ran = gh.ran.display(),
            rate_limit = gh.rate_limit.display(),
            held = gh.held.display(),
            says = gh.says.display()
        );
        let source = stack.path("gh.source");
        std::fs::write(&source, script).unwrap();
        install_executable(&source, &gh.bin.join("gh"));
        gh.say(NONE);
        gh.limit_left(5000);
        stack.add_env("PATH", format!("{}:{}", gh.bin.display(), std::env::var("PATH").unwrap()));
        // No login shell, so workers get this PATH, with the stand-in at its front.
        stack.add_env("SHELL", "/usr/bin/false".to_string());
        stack.start_hub();
        gh
    }

    fn say(&self, what: &str) {
        write_whole(&self.says, what);
    }

    /// What GitHub's limit has left of 5,000, as `gh api rate_limit` prints it, resetting in an hour.
    fn limit_left(&self, remaining: u32) {
        let in_an_hour = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 3600;
        let graphql = format!(r#"{{"limit":5000,"remaining":{remaining},"reset":{in_an_hour},"used":0}}"#);
        write_whole(&self.rate_limit, &format!(r#"{{"resources":{{"graphql":{graphql}}}}}"#));
    }

    /// How many times `gh pr view` has run, which is a call to GitHub each.
    fn views(&self) -> usize {
        self.ran("pr view")
    }

    /// How many times `gh` has run with these first two arguments.
    fn ran(&self, what: &str) -> usize {
        std::fs::read_to_string(&self.ran).unwrap_or_default().lines().filter(|line| *line == what).count()
    }

    fn hold(&self) {
        std::fs::write(&self.held, "").unwrap();
    }

    fn release(&self) {
        std::fs::remove_file(&self.held).unwrap();
    }
}

fn restart_hub(stack: &mut Stack) {
    stack.kill_hub();
    stack.start_hub();
}

/// Whole or not at all, since a worker may run the stand-in at any moment.
fn write_whole(path: &Path, text: &str) {
    let next = path.with_extension("next");
    std::fs::write(&next, text).unwrap();
    std::fs::rename(&next, path).unwrap();
}

/// The number of the pull request on the workspace's record, as every client gets it.
fn recorded(client: &Client, id: &WorkspaceId) -> Option<u64> {
    support::workspace(client, id).known_pull_request().map(|pr| pr.number)
}

#[test]
fn one_client_asking_records_the_pull_request_for_every_client_and_for_after_a_restart() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    gh.say(OPEN);

    let asked = client.workspace_pull_request(&workspace.id).unwrap().unwrap();
    gh.say(OFFLINE);
    restart_hub(&mut stack);
    let client = stack.client();

    assert_eq!((asked.number, asked.state, asked.url.as_str()), (7, PrState::Open, "https://example.com/7"));
    // GitHub is out of reach, so this is what the last hub found.
    assert_eq!(recorded(&client, &workspace.id), Some(7), "kept through the restart");
}

/// Offline, or logged out, says nothing about whether it's there, so it doesn't take away what was known. Only `gh`
/// saying there's none does.
#[test]
fn a_gh_that_cant_say_keeps_the_pull_request_known_and_one_that_says_none_clears_it() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    gh.say(OPEN);
    client.workspace_pull_request(&workspace.id).unwrap();

    gh.say(OFFLINE);
    let offline = client.workspace_pull_request(&workspace.id).unwrap();
    let kept = recorded(&client, &workspace.id);
    gh.say(NONE);
    let gone = client.workspace_pull_request(&workspace.id).unwrap();

    assert_eq!(offline.map(|pr| pr.number), Some(7));
    assert_eq!(kept, Some(7));
    assert_eq!(gone, None);
    let found = support::workspace(&client, &workspace.id).pull_request;
    assert_eq!(found, Some(PullRequestFound { pull_request: None }), "looked, and found none");
}

/// Logged out, it can't be read until the user logs in, so with nothing known it's taken to be none, which offers to
/// create one. Offline, it may well be there, so with nothing known the read fails, and it stays unread.
#[test]
fn with_nothing_known_a_gh_logged_out_says_none_and_one_offline_fails() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (offline, _) = stack.workspace(&client);
    let (logged_out, _) = stack.workspace(&client);

    gh.say(OFFLINE);
    let failed = client.workspace_pull_request(&offline.id);
    gh.say(LOGGED_OUT);
    let none = client.workspace_pull_request(&logged_out.id).unwrap();

    assert!(matches!(failed, Err(ClientError::Api(e)) if e.code == ErrorCode::Worker));
    assert_eq!(support::workspace(&client, &offline.id).pull_request, None, "still not looked at");
    assert_eq!(none, None);
    let found = support::workspace(&client, &logged_out.id).pull_request;
    assert_eq!(found, Some(PullRequestFound { pull_request: None }));
}

/// As after an update, when nobody has opened any workspace yet.
#[test]
fn a_starting_hub_reads_every_workspaces_pull_request_without_being_asked() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (first, _) = stack.workspace(&client);
    let (second, _) = stack.workspace(&client);
    gh.say(OPEN);

    restart_hub(&mut stack);
    let client = stack.client();

    wait_until("both to be read", || {
        recorded(&client, &first.id) == Some(7) && recorded(&client, &second.id) == Some(7)
    });
}

/// An agent may have opened one in its turn, so its end reads it, whether or not anyone is looking at the workspace.
/// It was just made, so nothing else reads it this soon.
#[test]
fn the_end_of_a_turn_reads_the_pull_request() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    gh.say(OPEN);

    client.session_prompt(&session.id, "say:opened one", None).unwrap();
    wait_for_turn_end(&client, &session.id, 1);

    wait_until("the pull request on its record", || recorded(&client, &workspace.id) == Some(7));
}

/// A workspace nobody has open has no worker once the hub starts again, and one started only to read its pull request
/// doesn't stay, so two hundred workspaces don't keep two hundred running. The next prompt starts one as usual.
#[test]
fn a_worker_started_only_to_read_a_pull_request_is_stopped_after() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (workspace, session) = stack.workspace(&client);
    gh.say(OPEN);

    restart_hub(&mut stack);
    let client = stack.client();
    wait_until("it to be read", || recorded(&client, &workspace.id) == Some(7));

    wait_until("its worker to stop", || client.hub_status().unwrap().workers == 0);
    client.session_prompt(&session.id, "say:still here", None).unwrap();
    assert!(support::assistant_text(&wait_for_turn_end(&client, &session.id, 1)).ends_with("still here"));
}

/// A prompt sent while a read nobody asked for has the worker it started takes the worker over, so it isn't stopped
/// under the prompt's agent once the read is done.
#[test]
fn a_worker_started_only_to_read_stays_once_a_prompt_wants_it() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (first, session) = stack.workspace(&client);
    let (second, _) = stack.workspace(&client);
    gh.say(OPEN);
    // Known to be open, so it's read before the second once the hub starts again.
    client.workspace_pull_request(&first.id).unwrap();
    gh.hold();

    restart_hub(&mut stack);
    let client = stack.client();
    wait_until("the first to be read again", || gh.views() == 2);
    client.session_prompt(&session.id, "hang:60000", None).unwrap();
    let mut agent = None;
    wait_until("the prompt's agent to start", || {
        agent = started_pid(&support::read_all(&client, &session.id));
        agent.is_some()
    });
    gh.release();
    // The second waits for the first's read to be done, worker and all.
    wait_until("the second to be read", || recorded(&client, &second.id) == Some(7));

    assert!(is_alive(agent.unwrap()), "the prompt's agent went with the read's worker");
}

/// A read nobody asked for asks GitHub what's left of its limit before spending any, and leaves the last fifth of it to
/// agents and to reads someone is waiting on.
#[test]
fn reads_nobody_asked_for_leave_the_last_of_githubs_limit_to_the_rest() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    gh.say(OPEN);
    gh.limit_left(900);
    let asked_before = gh.ran("api rate_limit");

    restart_hub(&mut stack);
    let client = stack.client();
    wait_until("GitHub to be asked what's left", || gh.ran("api rate_limit") > asked_before);
    wait_until("the worker started to ask to stop", || client.hub_status().unwrap().workers == 0);

    assert_eq!(gh.views(), 0, "it was read anyway");
    let asked = client.workspace_pull_request(&workspace.id).unwrap();
    assert_eq!(asked.map(|pr| pr.number), Some(7), "what's left is there for a read someone waits on");
}

/// GitHub would refuse it anyway, and each refused call is one more against the agents sharing the limit. With
/// nothing known, that fails, rather than saying there's none, which would offer to create one that may be there.
#[test]
fn with_githubs_limit_spent_a_read_says_what_is_known_without_asking_github() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    gh.limit_left(0);
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    gh.say(OPEN);

    let answer = client.workspace_pull_request(&workspace.id);

    assert!(matches!(answer, Err(ClientError::Api(e)) if e.code == ErrorCode::Unavailable));
    assert_eq!(gh.views(), 0, "nothing asked GitHub for it");
    assert_eq!(support::workspace(&client, &workspace.id).pull_request, None, "it still hasn't been looked at");
}

/// Asked for again while a read is on its way, the next read is still needed, since the first began before the ask.
/// But a third, asked for while the second waits, has nothing to add once the second has read.
#[test]
fn a_read_asked_for_while_another_waits_shares_its_answer() {
    let mut stack = Stack::start();
    let gh = StandInGh::start(&mut stack);
    let client = stack.client();
    let (workspace, _) = stack.workspace(&client);
    gh.say(OPEN);
    gh.hold();
    let socket = stack.socket();
    let ask = |id: WorkspaceId| {
        let socket = socket.clone();
        std::thread::spawn(move || Client::connect(&socket, "test").unwrap().workspace_pull_request(&id).unwrap())
    };
    let first = ask(workspace.id.clone());
    wait_until("the first to reach GitHub", || gh.views() == 1);
    let (second, third) = (ask(workspace.id.clone()), ask(workspace.id.clone()));
    // Each waiting, and the status asked for here.
    wait_until("both to wait", || client.hub_status().unwrap().requests == 4);

    gh.release();
    let answers: Vec<_> = [first, second, third].into_iter().map(|asked| asked.join().unwrap()).collect();

    assert!(answers.iter().all(|pr| pr.as_ref().map(|pr| pr.number) == Some(7)), "{answers:?}");
    assert_eq!(gh.views(), 2, "the third shared the second's read");
}
