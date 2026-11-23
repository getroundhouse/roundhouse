//! What a client holds of GitHub's observations, against a fake hub that sends them as a hub of each build does, and
//! what a view is told of them.

use std::time::Duration;

use protocol::client::github::GithubShown;
use protocol::client::listed::{ReferenceObservations, WorkspaceObservations};
use protocol::client::{HubNotice, ObservationNotice};
use protocol::model::{
    GithubLoginKind, GithubRepositoryName, IssueNumber, Local, Observation, ObservationKey, Observed, OwnPullRequest,
    OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary, PrState, PullRequestDetail, ReferenceAsk, ReferenceRead,
    RepositoryUnseen, Rev, SessionStatus, WorkspaceId,
};

use super::super::held::{GithubHeld, github_keys, notice_key};
use crate::fake_hub::{GithubSends, World, serve};
use crate::records::HubRecords;
use crate::{Client, GithubFeed, RecordsFeed};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Issue 12 of `example/project`, as a chip names it.
fn twelve() -> ReferenceAsk {
    ReferenceAsk {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(12).unwrap(),
    }
}

/// A world with the workspace `elm`, whose pull request is #7, open with a check running and its worktree a commit
/// ahead, the workspace `oak`, where GitHub showed no repository, and issue 12, served by a hub that sends GitHub as
/// `sends` says, with a client of it, and the directory its socket is in, which goes when the test ends.
fn served(sends: GithubSends) -> (tempfile::TempDir, Client) {
    let mut world = World::default();
    world.github_says.sends = sends;
    let repo = world.repo("aspen");
    let elm = world.workspace(&repo, "elm");
    let oak = world.workspace(&repo, "oak");
    world.chat(&elm, SessionStatus::Idle, Vec::new());
    let local = Local { uncommitted: 0, ahead: 1, behind: 0 };
    world
        .github_says
        .pull_requests
        .insert(elm, OwnPullRequest { pull_request: protocol::fixtures::pull_request(7), local });
    world.github_says.unseen.insert(oak, unseen());
    world.github_says.references.push(protocol::fixtures::issue("example/project", 12, "Decide"));
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub");
    serve(&socket, world);
    let client = Client::connect(&socket, "test").unwrap();
    (dir, client)
}

fn unseen() -> RepositoryUnseen {
    RepositoryUnseen {
        repository: GithubRepositoryName::named("example/project"),
        username: "ada".into(),
        login: GithubLoginKind::Token,
        installation: None,
        said: "Could not resolve to a Repository with the name 'example/project'.".into(),
        approve_at: None,
    }
}

/// What a view reads of each, once the hub has said it all, with the views that showed it, and the feed they read.
fn read_once_said(client: &Client) -> (Read, RecordsFeed, crate::GithubView) {
    let records = RecordsFeed::start(client);
    waiting::thread::until(TIMEOUT, "the hub's records", || records.connected());
    let feed = records.github();
    let view = feed.view();
    let workspaces = records.snapshot().workspaces().iter().map(|workspace| workspace.id.clone()).collect::<Vec<_>>();
    let elm = workspaces.iter().find(|id| records.snapshot().workspace(id).unwrap().name == "elm").unwrap().clone();
    view.show(GithubShown { workspaces: workspaces.clone(), closely: workspaces.clone(), references: vec![twelve()] });
    let read = || Read::of(&feed, &records, &elm);
    waiting::thread::until(TIMEOUT, "the pull request with its checks, the unseen repository and the chip", || {
        let read = read();
        read.checks.is_some() && read.unseen.is_some() && read.chip.is_some()
    });
    (read(), records, view)
}

/// What views read of GitHub, in the forms they read it in, and each of which form the hub sent.
#[derive(Clone, Debug, PartialEq)]
struct Read {
    state: Option<PrState>,
    local: Option<Local>,
    checks: Option<usize>,
    unseen: Option<RepositoryUnseen>,
    chip: Option<String>,
    said: bool,
}

impl Read {
    fn of(feed: &GithubFeed, records: &RecordsFeed, elm: &WorkspaceId) -> Self {
        let snapshot = records.snapshot();
        let workspace = |id: &WorkspaceId| snapshot.workspace(id).unwrap().clone();
        let oak = snapshot.workspaces().iter().find(|workspace| workspace.name == "oak").unwrap().clone();
        let own = feed.pull_request(&workspace(elm)).and_then(|found| found.pull_request);
        let chip = match feed.reference(&twelve()) {
            Some(ReferenceRead::Found { reference }) => reference.about().map(|about| about.title.clone()),
            _ => None,
        };
        Read {
            state: own.as_ref().map(|own| own.pull_request.said().state),
            local: own.as_ref().map(|own| own.local),
            checks: own.as_ref().and_then(|own| own.pull_request.checks_said()).map(|checks| checks.checks.len()),
            unseen: feed.unseen(&oak),
            chip,
            said: feed.said_pull_request(&workspace(elm)),
        }
    }
}

/// Which form the feed holds each in: the workspace's pull request, and the chip, each as (an observation, an update).
fn forms(feed: &GithubFeed, elm: &WorkspaceId) -> [(bool, bool); 2] {
    [
        feed.held_as(&ObservationKey::WorkspacePullRequest { workspace_id: elm.clone() }),
        feed.held_as(&ObservationKey::Reference { reference: twelve() }),
    ]
}

/// A view reads the same of GitHub whichever form its hub sends: as observations at every level from a hub of this
/// build, as `GithubUpdate` at every level from one from before them, and each form at its own levels from one that
/// sends only a chip as an observation. What's held of each thing is in the one form its hub sent.
#[test]
fn a_view_reads_the_same_of_github_whichever_form_its_hub_sends_it_in() {
    let mut read = Vec::new();
    for (sends, held) in [
        (GithubSends::Observations, [(true, false), (true, false)]),
        (GithubSends::Updates, [(false, true), (false, true)]),
        (GithubSends::ChipsObserved, [(false, true), (true, false)]),
    ] {
        let (_dir, client) = served(sends);
        let (said, records, _view) = read_once_said(&client);
        let elm = records.snapshot().workspaces().iter().find(|w| w.name == "elm").unwrap().id.clone();
        assert_eq!(forms(&records.github(), &elm), held, "{sends:?}");
        read.push(said);
    }

    let expected = Read {
        state: Some(PrState::Open),
        local: Some(Local { uncommitted: 0, ahead: 1, behind: 0 }),
        checks: Some(1),
        unseen: Some(unseen()),
        chip: Some("Decide".into()),
        said: true,
    };
    assert_eq!(read, vec![expected; 3]);
}

/// How `w`'s pull request stands, saying it's at `head` and stands as `state`, at `rev`.
fn stands(w: &str, head: &str, state: PrState, rev: u64) -> Observation {
    let mut own = OwnPullRequest { pull_request: protocol::fixtures::pull_request(7), local: Local::default() };
    own.pull_request.summary.said.head = head.into();
    own.pull_request.summary.said.state = state;
    Observation::WorkspacePullRequest {
        workspace_id: w.into(),
        pull_request: Observed::Current { value: OwnPullRequestSaid { pull_request: Some(OwnSummary::of(&own)) } },
        rev: Rev::new(rev),
    }
}

/// `w`'s pull request's checks, history and stack, of `head`, at `rev`.
fn detail(w: &str, head: &str, rev: u64) -> Observation {
    let mut pull_request = protocol::fixtures::pull_request(7);
    pull_request.summary.said.head = head.into();
    Observation::WorkspacePullRequestDetail {
        workspace_id: w.into(),
        detail: Observed::Current {
            value: OwnPullRequestDetail { pull_request: Some(PullRequestDetail::of(&pull_request)) },
        },
        rev: Rev::new(rev),
    }
}

/// A notice of `observation`.
fn notice(observation: Observation) -> HubNotice {
    ObservationNotice::Copy(observation).into()
}

/// What's held of GitHub and the copy of the hub's records beside it, taken in as the sources take them in.
#[derive(Default)]
struct Holding {
    records: HubRecords,
    held: GithubHeld,
}

impl Holding {
    fn notice(&mut self, observation: Observation) {
        let notice = notice(observation);
        let key = notice_key(&notice).unwrap();
        let changed = self.records.apply_notice(notice);
        self.held.observed(&[key], changed);
    }

    fn workspace_list(&mut self, w: &str, observations: Vec<Observation>, rev: u64) {
        let keys = github_keys(&observations);
        let listed = WorkspaceObservations { workspace_id: w.into(), observations, rev: Rev::new(rev) };
        let changed = self.records.apply_workspace_list(listed);
        self.held.observed(&keys, changed);
    }

    fn reference_list(&mut self, observations: Vec<Observation>, rev: u64) {
        let keys = github_keys(&observations);
        let listed = ReferenceObservations { reference: twelve(), observations, rev: Rev::new(rev) };
        let changed = self.records.apply_reference_list(listed);
        self.held.observed(&keys, changed);
    }

    fn pull_request(&self, w: &str) -> Option<OwnPullRequest> {
        let snapshot = self.records.snapshot();
        let observed = |key: &ObservationKey| snapshot.github_observation(key);
        self.held.pull_request(&w.into(), &observed).and_then(|found| found.pull_request)
    }
}

/// The checks, history and stack go with the pull request's summary only where they're of the head it's at, and are
/// otherwise none, which the bar draws as reading.
#[test]
fn the_detail_goes_with_the_summary_only_at_its_head() {
    let mut holding = Holding::default();
    holding.notice(stands("w", "head-8", PrState::Open, 5));
    holding.workspace_list("w", vec![detail("w", "head-7", 4)], 4);

    let pushed = holding.pull_request("w").unwrap();

    assert_eq!(pushed.pull_request.checks, None, "the checks of the commit before");
    assert_eq!((pushed.pull_request.history, pushed.pull_request.stack), (None, None));

    holding.notice(detail("w", "head-8", 6));

    let read = holding.pull_request("w").unwrap();
    assert!(read.pull_request.checks.is_some() && read.pull_request.stack.is_some());
}

/// The client's half of the issue's "Done when": two clients that follow the hub each hold the newer copy of how a
/// workspace's pull request stands, and keep it over an older one sent after.
#[test]
fn two_followers_hold_the_newer_copy_and_keep_it_over_an_older_one_sent_after() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    world.workspace(&repo, "elm");
    world.feed = vec![
        ObservationNotice::Copy(stands("elm", "head-7", PrState::Merged, 5)),
        ObservationNotice::Copy(stands("elm", "head-7", PrState::Open, 4)),
        // Last, so once it's taken in, so is every notice before it.
        ObservationNotice::Copy(stands("ash", "head-7", PrState::Open, 6)),
    ];
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub");
    serve(&socket, world);

    let followers: Vec<_> = (0..2).map(|_| RecordsFeed::start(&Client::connect(&socket, "test").unwrap())).collect();

    for records in &followers {
        let feed = records.github();
        let said = |w: &str| {
            let workspace = protocol::fixtures::workspace(w);
            feed.pull_request(&workspace).and_then(|found| found.pull_request).map(|own| own.pull_request.said().state)
        };
        waiting::thread::until(TIMEOUT, "the last notice", || said("ash").is_some());
        assert_eq!(said("elm"), Some(PrState::Merged), "the newer copy, though an older one came after it");
    }
}

/// A view is told of a change to an observation of what it shows, as it's told of a `GithubUpdate`, and isn't told of
/// one of what it doesn't show, or one that changes nothing.
#[test]
fn a_view_is_told_of_a_change_to_what_it_shows_and_of_nothing_else() {
    let mut holding = Holding::default();
    let (shows_w, w_told) = flume::unbounded();
    let (shows_chip, chip_told) = flume::unbounded();
    holding.held.add(1, shows_w);
    holding.held.add(2, shows_chip);
    holding.held.show(1, GithubShown { workspaces: vec!["w".into()], ..GithubShown::default() });
    holding.held.show(2, GithubShown { references: vec![twelve()], ..GithubShown::default() });
    let told = |told: &flume::Receiver<()>| told.drain().count();

    holding.notice(stands("w", "head-7", PrState::Open, 5));

    assert_eq!((told(&w_told), told(&chip_told)), (1, 0), "only the view that shows w");

    holding.notice(stands("w", "head-7", PrState::Open, 5));
    holding.notice(stands("v", "head-7", PrState::Open, 6));

    assert_eq!((told(&w_told), told(&chip_told)), (0, 0), "the same again, and another's");

    let chip = Observation::Reference {
        reference: twelve(),
        said: Observed::Current { value: protocol::model::ReferenceSaid::Missing },
        rev: Rev::new(7),
    };
    holding.reference_list(vec![chip], 7);

    assert_eq!((told(&w_told), told(&chip_told)), (0, 1), "only the view that shows the chip");
}
