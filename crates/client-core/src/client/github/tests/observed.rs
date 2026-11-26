//! What a client holds of GitHub's observations, against a fake hub that sends them as a hub does, as views read them,
//! and what tells a view which changed.

use std::time::Duration;

use protocol::client::listed::{ReferenceObservations, WorkspaceObservations};
use protocol::client::{HubNotice, ObservationNotice};
use protocol::model::{
    GithubLoginKind, GithubRepositoryName, IssueNumber, Local, Observation, Observed, OwnPullRequest,
    OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary, PrState, PullRequestDetail, ReferenceAsk, ReferenceSaid,
    RepositoryUnseen, Rev, SessionStatus, WorkspaceId,
};

use crate::fake_hub::{World, serve};
use crate::records::{HubRecords, Snapshot};
use crate::{Client, FollowedWorkspace, RecordsFeed};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Issue 12 of `example/project`, as a chip names it.
fn twelve() -> ReferenceAsk {
    ReferenceAsk {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(12).unwrap(),
    }
}

/// A world with the workspace `elm`, whose pull request is #7, open with a check running and its worktree a commit
/// ahead, the workspace `oak`, where GitHub showed no repository, and issue 12, with a client of a hub that serves it,
/// and the directory its socket is in, which goes when the test ends.
fn served() -> (tempfile::TempDir, Client) {
    let mut world = World::default();
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

/// What a view reads of each, once the hub has said it all, with every workspace followed at its own level, as the
/// handles on them say, the view that showed them, and the feed they read.
fn read_once_said(client: &Client) -> (Read, RecordsFeed, (Vec<FollowedWorkspace>, crate::GithubView)) {
    let records = RecordsFeed::start(client);
    waiting::thread::until(TIMEOUT, "the hub's records", || records.connected());
    let feed = records.github();
    let view = feed.view();
    let workspaces = records.snapshot().workspaces().iter().map(|workspace| workspace.id.clone()).collect::<Vec<_>>();
    let elm = workspaces.iter().find(|id| records.snapshot().workspace(id).unwrap().name == "elm").unwrap().clone();
    let followed = workspaces.iter().map(|id| records.follow_workspace(id)).collect();
    view.show(vec![twelve()]);
    let read = || Read::of(&records.snapshot(), &elm);
    waiting::thread::until(TIMEOUT, "the pull request with its checks, the unseen repository and the chip", || {
        let read = read();
        read.checks.is_some() && read.unseen.is_some() && read.chip.is_some()
    });
    (read(), records, (followed, view))
}

/// What views read of GitHub, and whether the hub said the pull request is kept current.
#[derive(Clone, Debug, PartialEq)]
struct Read {
    state: Option<PrState>,
    local: Option<Local>,
    checks: Option<usize>,
    unseen: Option<RepositoryUnseen>,
    chip: Option<String>,
    current: bool,
}

impl Read {
    fn of(snapshot: &Snapshot, elm: &WorkspaceId) -> Self {
        let oak = snapshot.workspaces().iter().find(|workspace| workspace.name == "oak").unwrap().id.clone();
        let said = snapshot.workspace_pull_request(elm);
        let own = said_of(&said).and_then(Option::as_ref);
        let detail = own.and_then(|own| said_of(&own.detail));
        Read {
            state: own.map(|own| own.own.pull_request.summary.state),
            local: own.map(|own| own.own.local),
            checks: detail.and_then(|detail| detail.checks.as_ref()).map(|checks| checks.checks.len()),
            unseen: said_of(snapshot.workspace_repository_unseen(&oak)).cloned().flatten(),
            chip: said_of(snapshot.reference_said(&twelve()))
                .and_then(ReferenceSaid::about)
                .map(|about| about.title.clone()),
            current: matches!(said, Observed::Current { .. }),
        }
    }
}

/// What `observed` says, however fresh.
fn said_of<T>(observed: &Observed<T>) -> Option<&T> {
    match observed {
        Observed::Current { value } | Observed::AsOf { value, .. } => Some(value),
        Observed::NotKnown | Observed::Unknown => None,
    }
}

/// A view reads what GitHub says of a workspace's own pull request, of a repository GitHub showed none of, and of a
/// chip, as the hub's observations of each say, once they've come.
#[test]
fn a_view_reads_github_from_the_hubs_observations() {
    let (_dir, client) = served();

    let (read, _records, _shown) = read_once_said(&client);

    let expected = Read {
        state: Some(PrState::Open),
        local: Some(Local { uncommitted: 0, ahead: 1, behind: 0 }),
        checks: Some(1),
        unseen: Some(unseen()),
        chip: Some("Decide".into()),
        current: true,
    };
    assert_eq!(read, expected);
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

/// The copy of the hub's records, taking in what each level sends of GitHub as the sources take it in.
#[derive(Default)]
struct Holding {
    records: HubRecords,
}

impl Holding {
    fn notice(&mut self, observation: Observation) {
        self.records.apply_notice(notice(observation));
    }

    fn workspace_list(&mut self, w: &str, observations: Vec<Observation>, rev: u64) {
        let listed = WorkspaceObservations { workspace_id: w.into(), observations, rev: Rev::new(rev) };
        self.records.apply_workspace_list(listed);
    }

    fn reference_list(&mut self, observations: Vec<Observation>, rev: u64) {
        let listed = ReferenceObservations { reference: twelve(), observations, rev: Rev::new(rev) };
        self.records.apply_reference_list(listed);
    }

    /// The checks, history and stack read with `w`'s pull request, and how fresh they are.
    fn detail(&self, w: &str) -> Observed<PullRequestDetail> {
        let snapshot = self.records.snapshot();
        let Observed::Current { value: Some(own) } = snapshot.workspace_pull_request(&w.into()) else {
            panic!("w's pull request is current")
        };
        match own.detail {
            Observed::Current { value } => Observed::Current { value: value.clone() },
            Observed::AsOf { value, as_of_ms } => Observed::AsOf { value: value.clone(), as_of_ms },
            Observed::NotKnown => Observed::NotKnown,
            Observed::Unknown => Observed::Unknown,
        }
    }
}

/// The checks, history and stack go with the pull request's summary only where they're of the head it's at, and are
/// otherwise none, which the bar draws as reading.
#[test]
fn the_detail_goes_with_the_summary_only_at_its_head() {
    let mut holding = Holding::default();
    holding.notice(stands("w", "head-8", PrState::Open, 5));
    holding.workspace_list("w", vec![detail("w", "head-7", 4)], 4);

    assert_eq!(holding.detail("w"), Observed::NotKnown, "the checks of the commit before");

    holding.notice(detail("w", "head-8", 6));

    let Observed::Current { value: read } = holding.detail("w") else { panic!("{:?}", holding.detail("w")) };
    assert!(read.checks.is_some() && read.stack.is_some());
}

/// What a view last looked at tells it whether what the hub says of a chip has changed since, without comparing what
/// it says, and only of the chip it changed.
#[test]
fn a_view_tells_which_chip_changed_since_it_last_looked() {
    let mut holding = Holding::default();
    let said = |rev: u64| Observation::Reference {
        reference: twelve(),
        said: Observed::Current { value: ReferenceSaid::Missing },
        rev: Rev::new(rev),
    };
    let thirteen = ReferenceAsk { number: IssueNumber::try_from(13).unwrap(), ..twelve() };
    let before = holding.records.snapshot();

    holding.reference_list(vec![said(7)], 7);

    let after = holding.records.snapshot();
    let changes = after.changes_since(&before);
    assert_eq!((changes.reference_said(&twelve()), changes.reference_said(&thirteen)), (true, false));

    holding.notice(said(7));

    assert!(!holding.records.snapshot().changes_since(&after).reference_said(&twelve()), "the same again");
}
