//! What a client of version 4 that doesn't read GitHub as observations is sent of what GitHub says of what it follows,
//! as `HubFrame::Github`, over a connection in memory, as one on the hub's socket is served.

use std::sync::Arc;
use std::time::Duration;

use github::{PullRequestState, References};
use protocol::client::github::GithubUpdate;
use protocol::client::subscriptions::{Follow, Subscribe, Subscribed, Subscription};
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, ErrorCode, Hello, HubFrame, ReplyOutcome, Request};
use protocol::frame;
use protocol::model::{
    About, GithubRepositoryName, Issue, IssueNumber, IssueState, IssueSummary, PrState, PullRequest, Read, Reference,
    ReferenceAsk, ReferenceRead, WorkspaceId,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};
use tokio::time::timeout;

use crate::hub::tests::{archived, forget_github, forget_github_but_a_summary, hub_on_github_with};
use crate::hub::{Followed, Hub};
use crate::server::admit::Gate;
use crate::server::connection::serve_connection;

/// Long enough for anything here on a busy machine.
const WAIT: Duration = Duration::from_secs(10);

/// A client's connection to a hub, served as one on its socket is, with what the hub has sent of GitHub.
struct Client {
    frames: Lines<BufReader<ReadHalf<DuplexStream>>>,
    sending: WriteHalf<DuplexStream>,
    next_call: u64,
    /// What the hub has sent of what's shown, in order.
    updates: Vec<GithubUpdate>,
}

impl Client {
    /// A client that has said hello to `hub` at version 4.
    async fn of(hub: &Arc<Hub>) -> Self {
        let (client, served) = tokio::io::duplex(1 << 16);
        let (read, write) = tokio::io::split(served);
        tokio::spawn(serve_connection(hub.clone(), read, write, Gate::local(hub)));
        let (frames, sending) = tokio::io::split(client);
        let mut client = Self { frames: BufReader::new(frames).lines(), sending, next_call: 0, updates: Vec::new() };
        let hello = Hello { protocol_version: 4, client: "test".into(), oldest_protocol_version: None, device: None };
        client.call(ClientRequest::Hello(hello)).await.unwrap();
        client
    }

    /// Sends `request`, and gives back the hub's reply, keeping what it sends of GitHub meanwhile.
    async fn call(&mut self, request: ClientRequest) -> Result<ClientResponse, ErrorCode> {
        self.next_call += 1;
        let id = self.next_call;
        let line = frame::encode(&ClientFrame { id, request }).unwrap();
        self.sending.write_all(&line).await.unwrap();
        loop {
            match self.next().await {
                HubFrame::Reply { id: replied, outcome } if replied == id => {
                    return match outcome {
                        ReplyOutcome::Response(response) => Ok(response),
                        ReplyOutcome::Error(error) => Err(error.code),
                    };
                }
                frame @ (HubFrame::Reply { .. }
                | HubFrame::Event { .. }
                | HubFrame::Tail { .. }
                | HubFrame::Delta { .. }
                | HubFrame::Notice { .. }
                | HubFrame::Github { .. }
                | HubFrame::Terminal { .. }
                | HubFrame::TerminalPicture { .. }
                | HubFrame::Listed { .. }) => self.keep(frame),
            }
        }
    }

    /// Stops each of `stop`, and follows `follow` as subscription `subscription_id`, reading GitHub as
    /// `HubFrame::Github`.
    async fn follow(&mut self, stop: Vec<u64>, subscription_id: u64, follow: Follow) {
        let start = vec![Subscription::new(subscription_id, follow)];
        let reply = self.call(Subscribe { stop, start }.into_request()).await;
        assert_eq!(reply, Ok(ClientResponse::Subscribed(Subscribed::default())));
    }

    /// Reads what the hub sends until what it has sent of GitHub holds `done`.
    async fn until(&mut self, what: &str, done: impl Fn(&[GithubUpdate]) -> bool) {
        while !done(&self.updates) {
            let frame = timeout(WAIT, self.next())
                .await
                .unwrap_or_else(|_| panic!("waiting for {what}, the hub sent only {:#?}", self.updates));
            self.keep(frame);
        }
    }

    async fn next(&mut self) -> HubFrame {
        let line = self.frames.next_line().await.unwrap().expect("the hub closed the connection");
        frame::decode(line.as_bytes()).unwrap()
    }

    fn keep(&mut self, frame: HubFrame) {
        if let HubFrame::Github { update, .. } = frame {
            self.updates.push(update);
        }
    }
}

fn number(n: u64) -> IssueNumber {
    IssueNumber::try_from(n).unwrap()
}

/// The issue or pull request numbered `n` in `example/project`, the repository the hub's test workspace is on.
fn own(n: u64) -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("example/project"), number: number(n) }
}

/// An open issue numbered `n` in `example/project`, titled `title`.
fn issue(n: u64, title: &str) -> Reference {
    let about = About {
        url: format!("https://github.com/example/project/issues/{n}"),
        title: title.to_string(),
        author: Some("ada".into()),
        opened_at_ms: 1,
        comments: 0,
        labels: Vec::new(),
        body: String::new(),
    };
    Reference::Issue(Issue {
        repository: GithubRepositoryName::named("example/project"),
        number: number(n),
        summary: Read { said: IssueSummary { about, state: IssueState::Open }, read_at_ms: 1 },
    })
}

/// What the hub last sent of `ask`.
fn reference(updates: &[GithubUpdate], ask: &ReferenceAsk) -> Option<ReferenceRead> {
    updates.iter().rev().find_map(|update| {
        let GithubUpdate::Reference { reference, read } = update else { return None };
        (reference == ask).then(|| read.clone())
    })
}

/// What the hub last sent of `workspace`'s pull request.
fn pull_request(updates: &[GithubUpdate], workspace: &str) -> Option<PullRequest> {
    let workspace = WorkspaceId::from(workspace);
    updates.iter().rev().find_map(|update| {
        let GithubUpdate::PullRequest { workspace: of, found, .. } = update else { return None };
        if *of != workspace {
            return None;
        }
        found.as_ref().and_then(|found| found.pull_request.clone()).map(|own| own.pull_request)
    })
}

fn titled(read: Option<ReferenceRead>) -> Option<String> {
    match read {
        Some(ReferenceRead::Found { reference }) => reference.about().map(|about| about.title.clone()),
        _ => None,
    }
}

/// A client that follows something is sent what the hub reads of it, without asking for it, and then each change, as
/// the hub reads it for whatever reason, such as an agent's turn ending.
#[tokio::test]
async fn a_client_is_sent_what_it_follows_and_then_what_changes_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue(12, "Decide"));
    let (hub, pull_requests, _github) = hub_on_github_with(dir.path(), &references).await;
    pull_requests.set(7, PullRequestState::Open);
    let mut client = Client::of(&hub).await;

    client.follow(Vec::new(), 1, Follow::Hub).await;
    client.follow(Vec::new(), 2, Follow::Reference { reference: own(12) }).await;
    // Which has the hub read its pull request, as the pull request's bar does.
    client.follow(Vec::new(), 3, Follow::Workspace { id: "w".into() }).await;
    client
        .until("the issue and the pull request", |sent| {
            titled(reference(sent, &own(12))).is_some() && pull_request(sent, "w").is_some()
        })
        .await;
    pull_requests.set(7, PullRequestState::Merged);
    hub.workspace_pull_request("w".into()).await.unwrap();

    client
        .until("the pull request merged", |sent| {
            pull_request(sent, "w").is_some_and(|pr| pr.said().state == PrState::Merged)
        })
        .await;
    assert_eq!(titled(reference(&client.updates, &own(12))).as_deref(), Some("Decide"));
}

/// What a subscription stopped followed is followed no longer, and what's newly followed is sent.
#[tokio::test]
async fn what_a_stopped_subscription_followed_is_followed_no_longer() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue(12, "Decide"));
    references.set(issue(13, "Build"));
    let (hub, _pull_requests, _github) = hub_on_github_with(dir.path(), &references).await;
    let mut client = Client::of(&hub).await;

    client.follow(Vec::new(), 1, Follow::Reference { reference: own(12) }).await;
    client.until("the first issue", |sent| titled(reference(sent, &own(12))).is_some()).await;
    client.follow(vec![1], 2, Follow::Reference { reference: own(13) }).await;
    client.until("the second issue", |sent| titled(reference(sent, &own(13))).is_some()).await;

    assert!(!hub.followed(&Followed::Reference(own(12))), "the first, once its subscription has stopped");
    assert!(hub.followed(&Followed::Reference(own(13))));
}

/// What a client follows ends with its connection, so the hub stops reading it for a client that's gone.
#[tokio::test]
async fn what_a_client_follows_ends_with_its_connection() {
    let dir = tempfile::tempdir().unwrap();
    let references = References::default();
    references.set(issue(12, "Decide"));
    let (hub, _pull_requests, _github) = hub_on_github_with(dir.path(), &references).await;
    let mut client = Client::of(&hub).await;
    client.follow(Vec::new(), 1, Follow::Reference { reference: own(12) }).await;
    assert!(hub.followed(&Followed::Reference(own(12))));

    drop(client);

    waiting::task::until(WAIT, "what the client showed to end", || !hub.followed(&Followed::Reference(own(12)))).await;
}

/// A workspace whose pull request the hub hasn't looked for is sent as that too, so a client that heard of one on a
/// connection before, as from a hub before a restart, doesn't keep showing it.
#[tokio::test]
async fn a_workspace_not_looked_at_yet_is_sent_as_that() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _pull_requests, _github) = hub_on_github_with(dir.path(), &References::default()).await;
    let mut client = Client::of(&hub).await;

    client.follow(Vec::new(), 1, Follow::Hub).await;

    client
        .until("that the hub hasn't looked", |sent| {
            sent.iter().any(|update| {
                matches!(update, GithubUpdate::PullRequest { workspace, found: None, .. } if *workspace == WorkspaceId::from("w"))
            })
        })
        .await;
}

/// An archived workspace, which the hub doesn't read in turn, whose pull request the cache has forgotten, as after it
/// was emptied, is read by the number its record links to once a client shows it, and sent as GitHub says it is now.
#[tokio::test]
async fn an_archived_workspaces_pull_request_the_cache_forgot_is_read_again_once_shown() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github_with(dir.path(), &References::default()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    archived(&hub);
    forget_github(&hub);
    pull_requests.set(7, PullRequestState::Merged);
    let mut client = Client::of(&hub).await;

    client.follow(Vec::new(), 1, Follow::Hub).await;

    client
        .until("the pull request, merged", |sent| {
            pull_request(sent, "w").is_some_and(|pr| pr.said().state == PrState::Merged)
        })
        .await;
    assert_eq!(linked_to(&hub).await, Some(7), "the record keeps its link");
}

/// One the cache has only the summary of, as a chip's read keeps it, is read again too, since what the bar shows needs
/// its checks, and sent whole.
#[tokio::test]
async fn an_archived_workspaces_pull_request_the_cache_has_only_the_summary_of_is_read_again_once_shown() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, _github) = hub_on_github_with(dir.path(), &References::default()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    archived(&hub);
    forget_github_but_a_summary(&hub);
    pull_requests.set(7, PullRequestState::Merged);
    let mut client = Client::of(&hub).await;

    client.follow(Vec::new(), 1, Follow::Hub).await;

    client
        .until("the pull request, merged, with its checks", |sent| {
            pull_request(sent, "w").is_some_and(|pr| pr.said().state == PrState::Merged && pr.checks.is_some())
        })
        .await;
}

/// One whose first read fails, as while GitHub is down, is tried again while a client follows the hub, until it's
/// read.
#[tokio::test]
async fn an_archived_workspaces_pull_request_that_couldnt_be_read_again_is_tried_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, pull_requests, github) = hub_on_github_with(dir.path(), &References::default()).await;
    pull_requests.set(7, PullRequestState::Open);
    hub.workspace_pull_request("w".into()).await.unwrap();
    archived(&hub);
    forget_github(&hub);
    pull_requests.set(7, PullRequestState::Merged);
    pull_requests.fail(true);
    let asked = github.asked().len();
    tokio::spawn(hub.clone().watch_followed_references());
    let mut client = Client::of(&hub).await;

    client.follow(Vec::new(), 1, Follow::Hub).await;
    waiting::task::until(WAIT, "a read that fails", || github.asked().len() > asked).await;
    pull_requests.fail(false);

    client
        .until("the pull request, merged", |sent| {
            pull_request(sent, "w").is_some_and(|pr| pr.said().state == PrState::Merged)
        })
        .await;
    assert_eq!(linked_to(&hub).await, Some(7), "the record keeps its link");
}

/// The number of the pull request the record of the workspace `w` links to.
async fn linked_to(hub: &Arc<Hub>) -> Option<u64> {
    let workspace = hub.workspace(&"w".into()).await.unwrap();
    workspace.pull_request_linked().map(|link| link.number.get())
}
