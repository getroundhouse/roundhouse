//! The connection to the hub this phone paired with, which client-core's [`Link`] makes again whenever it goes, the
//! records it keeps from it, with what the hub says of the pull requests of the workspaces the phone lists, and what
//! has the hub keep what's typed and not sent.

use std::sync::Arc;

use client_core::link::{Connect, Link, Lost, Refused};
use client_core::records::Snapshot;
use client_core::{Client, ClientError, Drafts, GithubView, RecordsFeed, RemoteHub, ShownOfGithub};
use gpui_kit::{App, Context, EventEmitter, Task};
use protocol::model::{PullRequestFound, Workspace, WorkspaceStatus};

/// What the hub's list of clients calls the app.
const CLIENT_NAME: &str = "roundhouse-ios";

/// How the app reaches the hub it paired with. It's given to the app as it opens, so a test can have it connect to a
/// hub of its own, such as the gallery's, which answers from fixed data on a socket.
pub(crate) type ConnectTo = Arc<dyn Fn(&RemoteHub) -> Result<Client, ClientError> + Send + Sync>;

/// As the app connects on a phone: over the network, to whichever of the hub's addresses answers first.
pub(crate) fn over_the_network() -> ConnectTo {
    Arc::new(|hub| Client::connect_remote(hub, CLIENT_NAME))
}

/// How the connection stands, as the phone says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkState {
    Connecting,
    /// Connected, and client-core has caught up on the hub's records.
    Ready,
    /// The last try failed, with what went wrong. Another is on its way.
    Unreachable(String),
    /// The last try failed in a way trying again can't fix, such as the hub having revoked the phone's token, with what
    /// went wrong. The phone tries again only when it comes back to the front, so forgetting the hub and pairing again
    /// is the way on.
    Refused(String),
}

/// A new connection, which whatever was following the last one carries on with.
pub(crate) struct Reconnected(pub(crate) Client);

pub(crate) struct HubLink {
    hub: RemoteHub,
    link: Link<PairedHub>,
    state: LinkState,
    /// The connection, while there is one that's caught up.
    client: Option<Client>,
    /// The pull requests of the workspaces listed, which the hub sends as they change, once the link has the hub's
    /// records, and the task that draws them again.
    marks: Option<(GithubView, Task<()>)>,
    /// What has the hub keep what's typed and not sent, from the first connection on, and the task that shows a draft
    /// made elsewhere once one typed here has reached the hub.
    drafts: Option<(Drafts, Task<()>)>,
    _following: Task<()>,
}

/// The hub as the phone paired with it, which the link connects to as the app was given.
struct PairedHub {
    hub: RemoteHub,
    connect: ConnectTo,
}

impl Connect for PairedHub {
    type With = ();
    type Failure = String;

    fn connect(&mut self) -> Result<(Client, ()), Refused<String>> {
        (self.connect)(&self.hub).map(|client| (client, ())).map_err(|e| Refused::by(&e, e.to_string()))
    }

    fn lost(&self, lost: &Lost) -> String {
        lost.to_string()
    }
}

/// Whether the phone lists `workspace`: ready ones only, since one still being made, being archived or archived has
/// nothing to open here. The desktop's sidebar also lists one being archived, and notes one being made.
pub(crate) fn listed(workspace: &Workspace) -> bool {
    workspace.status == WorkspaceStatus::Ready
}

/// What a screen that works on the hub needs of it, while there's a connection that's caught up.
#[derive(Clone)]
pub(crate) struct Ready {
    pub(crate) client: Client,
    pub(crate) feed: RecordsFeed,
    pub(crate) drafts: Drafts,
}

impl EventEmitter<Reconnected> for HubLink {}

impl HubLink {
    pub(crate) fn new(hub: RemoteHub, connect: ConnectTo, cx: &mut Context<Self>) -> HubLink {
        let link = start(PairedHub { hub: hub.clone(), connect }, cx);
        let changed = link.changed().clone();
        let following = cx.spawn(async move |this, cx| {
            while changed.recv_async().await.is_ok() {
                if this.update(cx, Self::link_changed).is_err() {
                    return;
                }
            }
        });
        HubLink {
            hub,
            link,
            state: LinkState::Connecting,
            client: None,
            marks: None,
            drafts: None,
            _following: following,
        }
    }

    pub(crate) fn hub(&self) -> &RemoteHub {
        &self.hub
    }

    pub(crate) fn state(&self) -> &LinkState {
        &self.state
    }

    /// The connection, while there is one that's caught up.
    pub(crate) fn client(&self) -> Option<Client> {
        self.client.clone()
    }

    /// What client-core holds of the hub's records, once it has followed them at all.
    pub(crate) fn snapshot(&self) -> Option<Snapshot> {
        self.link.feed().map(|feed| feed.snapshot())
    }

    /// `workspace`'s pull request, as the hub sent it. `None` until the hub has looked.
    pub(crate) fn pull_request_of(&self, workspace: &Workspace) -> Option<PullRequestFound> {
        self.marks.as_ref()?.0.feed().pull_request(workspace)
    }

    /// The connection, the records and the drafts, while there's a connection that's caught up.
    pub(crate) fn ready(&self) -> Option<Ready> {
        let (client, feed, (drafts, _)) = (self.client()?, self.link.feed()?, self.drafts.as_ref()?);
        Some(Ready { client, feed, drafts: drafts.clone() })
    }

    /// Connects again now, such as when the app comes back to the front, since a phone that slept may hold a
    /// connection that looks open but has gone.
    pub(crate) fn connect_now(&mut self, _: &mut Context<Self>) {
        self.link.connect_now();
    }

    /// Has the hub keep at once what's typed and not kept yet, rather than once typing pauses, for an app iOS may end
    /// without a word, such as one sent to the background.
    pub(crate) fn keep_drafts_now(&self) {
        if let Some((drafts, _)) = &self.drafts {
            // How it went isn't waited for: the app goes on as it was either way.
            drop(drafts.flushed());
        }
    }

    /// Draws how the link stands, hands any new connection on, and shows the hub which workspaces are listed.
    fn link_changed(&mut self, cx: &mut Context<Self>) {
        if let Some((client, ())) = self.link.take_connection() {
            self.keep_drafts_on(&client, cx);
            cx.emit(Reconnected(client));
        }
        let state = self.link.state();
        self.client = state.ready().cloned();
        self.state = match state {
            client_core::link::LinkState::Connecting(_) | client_core::link::LinkState::CatchingUp(_) => {
                LinkState::Connecting
            }
            client_core::link::LinkState::Ready(_) => LinkState::Ready,
            client_core::link::LinkState::Waiting(why) => LinkState::Unreachable(why),
            client_core::link::LinkState::Stopped(why) => LinkState::Refused(why),
        };
        // Following goes on whatever went wrong while it did, which nothing here shows.
        drop(self.link.take_errors());
        if let Some(feed) = self.link.feed() {
            let (marks, _) = self.marks.get_or_insert_with(|| hear(&feed, cx));
            let listed = feed.snapshot().workspaces().iter().filter(|w| listed(w)).map(|w| w.id.clone()).collect();
            marks.show(ShownOfGithub { workspaces: listed, references: Vec::new() });
        }
        cx.notify();
    }

    /// Keeps drafts on `client`'s hub from here on, carrying on with any the last connection couldn't.
    fn keep_drafts_on(&mut self, client: &Client, cx: &mut Context<Self>) {
        if let Some((drafts, _)) = &self.drafts {
            drafts.reconnect(client);
        } else {
            let drafts = Drafts::start(client);
            let shown = show_drafts_from_elsewhere(&drafts, cx);
            self.drafts = Some((drafts, shown));
        }
    }
}

/// Follows what the hub says of the pull requests of the workspaces shown, drawing them again as they change.
fn hear(feed: &RecordsFeed, cx: &mut Context<HubLink>) -> (GithubView, Task<()>) {
    let marks = feed.github().view();
    let changed = marks.changed().clone();
    let hearing = cx.spawn(async move |this, cx| {
        while changed.recv_async().await.is_ok() {
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                return;
            }
        }
    });
    (marks, hearing)
}

/// Shows the records again whenever no draft has anything of this client's on its way, so a draft made elsewhere,
/// which waited behind one typed here, is shown, as `client_core::Drafts::from_elsewhere` says.
fn show_drafts_from_elsewhere(drafts: &Drafts, cx: &mut Context<HubLink>) -> Task<()> {
    let caught_up = drafts.caught_up().clone();
    cx.spawn(async move |this, cx| {
        while caught_up.recv_async().await.is_ok() {
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                return;
            }
        }
    })
}

#[cfg(not(any(test, feature = "testing")))]
fn start(hub: PairedHub, _: &App) -> Link<PairedHub> {
    Link::start(hub)
}

/// Starts the link, waiting before trying again as a test says, if one has.
#[cfg(any(test, feature = "testing"))]
fn start(hub: PairedHub, cx: &App) -> Link<PairedHub> {
    match cx.try_global::<StandIn>() {
        Some(StandIn(waits)) => Link::start_waiting(hub, *waits),
        None => Link::start(hub),
    }
}

/// The waits a test has the link take in place of every client's.
#[cfg(any(test, feature = "testing"))]
struct StandIn(client_core::link::Waits);

#[cfg(any(test, feature = "testing"))]
impl gpui_kit::Global for StandIn {}

/// Has each link made from now on wait as `waits` says before trying again. The waits are by the system's clock rather
/// than a GPUI timer, since client-core makes them, so a test can't move them, and would otherwise sit through them.
#[cfg(any(test, feature = "testing"))]
pub fn the_retry_waits_are(waits: client_core::link::Waits, cx: &mut App) {
    cx.set_global(StandIn(waits));
}
