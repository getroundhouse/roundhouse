//! The connection to the hub this phone paired with, which client-core's [`Link`] makes again whenever it goes, the
//! records it keeps from it, with what the hub says of each workspace's pull request, and what has the hub keep what's
//! typed and not sent.

use std::sync::Arc;

use client_core::link::{Connect, Link, Lost, Refused, Waits};
use client_core::records::Snapshot;
use client_core::{Client, ClientError, Drafts, Hub, OlderSide, RecordsFeed, RemoteHub};
use client_ui::{NeedsUpdating, needs_updating};
use gpui_kit::{Context, EventEmitter, Task};
use protocol::model::{Workspace, WorkspaceStatus};

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
    /// The hub is older than this app and shares no protocol version with it, with what to update. Another try is on
    /// its way, in case it has been.
    HubOlder(String),
    /// The last try failed in a way trying again can't fix, such as the hub having revoked the phone's token, with what
    /// went wrong. The phone tries again only when it comes back to the front, so forgetting the hub and pairing again
    /// is the way on.
    Refused(String),
}

/// A new connection, which whatever was following the last one carries on with.
pub(crate) struct Reconnected(pub(crate) Client);

pub(crate) struct HubLink {
    paired: RemoteHub,
    link: Link<PairedHub>,
    /// The hub the link connects to, as every screen of it holds it: its records, and what's asked of it, kept over
    /// every connection.
    hub: Hub,
    state: LinkState,
    /// The connection, while there is one that's caught up.
    client: Option<Client>,
    /// What has the hub keep what's typed and not sent, from the first connection on, and the task that shows a draft
    /// made elsewhere once one typed here has reached the hub.
    drafts: Option<(Drafts, Task<()>)>,
    _following: Task<()>,
}

/// Why the phone isn't connected, as it says it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Failure {
    /// The hub is older than this app, across a protocol break.
    HubOlder(String),
    Other(String),
}

impl Failure {
    /// What `error` says of `hub`.
    fn of(hub: &RemoteHub, error: &ClientError) -> Failure {
        let Some(older) = error.older_side() else { return Failure::Other(error.to_string()) };
        let NeedsUpdating { heading, says } = needs_updating(&hub.hub_name, "iPhone", None, &older);
        let why = format!("{heading}. {says}");
        match older {
            OlderSide::Hub { .. } => Failure::HubOlder(why),
            OlderSide::Client { .. } => Failure::Other(format!("{why} Update Roundhouse on this iPhone to reconnect.")),
        }
    }
}

/// The hub as the phone paired with it, which the link connects to as the app was given.
struct PairedHub {
    hub: RemoteHub,
    connect: ConnectTo,
}

impl Connect for PairedHub {
    type With = ();
    type Failure = Failure;

    fn connect(&mut self) -> Result<(Client, ()), Refused<Failure>> {
        (self.connect)(&self.hub).map(|client| (client, ())).map_err(|e| Refused::by(&e, Failure::of(&self.hub, &e)))
    }

    fn lost(&self, lost: &Lost) -> Failure {
        Failure::Other(lost.to_string())
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
    pub(crate) hub: Hub,
    pub(crate) client: Client,
    pub(crate) feed: RecordsFeed,
    pub(crate) drafts: Drafts,
}

impl EventEmitter<Reconnected> for HubLink {}

impl HubLink {
    pub(crate) fn new(paired: RemoteHub, connect: ConnectTo, cx: &mut Context<Self>) -> HubLink {
        let waits = cx.default_global::<RetryWaits>().0;
        let link = Link::start_waiting(PairedHub { hub: paired.clone(), connect }, waits);
        let changed = link.changed().clone();
        let following = cx.spawn(async move |this, cx| {
            while changed.recv_async().await.is_ok() {
                if this.update(cx, Self::link_changed).is_err() {
                    return;
                }
            }
        });
        let hub = link.hub();
        HubLink { paired, link, hub, state: LinkState::Connecting, client: None, drafts: None, _following: following }
    }

    /// The hub as the phone paired with it.
    pub(crate) fn paired(&self) -> &RemoteHub {
        &self.paired
    }

    pub(crate) fn hub(&self) -> &Hub {
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
        self.hub.records().map(|feed| feed.snapshot())
    }

    /// The connection, the records and the drafts, while there's a connection that's caught up.
    pub(crate) fn ready(&self) -> Option<Ready> {
        let (client, feed, (drafts, _)) = (self.client()?, self.hub.records()?, self.drafts.as_ref()?);
        Some(Ready { hub: self.hub.clone(), client, feed, drafts: drafts.clone() })
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

    /// Draws how the link stands, and what it has of the hub, and hands any new connection on.
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
            client_core::link::LinkState::Waiting(Failure::HubOlder(why)) => LinkState::HubOlder(why),
            client_core::link::LinkState::Waiting(Failure::Other(why)) => LinkState::Unreachable(why),
            client_core::link::LinkState::Stopped(Failure::HubOlder(why) | Failure::Other(why)) => {
                LinkState::Refused(why)
            }
        };
        // Following goes on whatever went wrong while it did, which nothing here shows.
        drop(self.link.take_errors());
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

/// The waits each link made takes before trying again: every client's, unless a test has given others.
struct RetryWaits(Waits);

impl gpui_kit::Global for RetryWaits {}

impl Default for RetryWaits {
    fn default() -> Self {
        Self(Waits::EVERY_CLIENT)
    }
}

/// Has each link made from now on wait as `waits` says before trying again. The waits are by the system's clock rather
/// than a GPUI timer, since client-core makes them, so a test can't move them, and would otherwise sit through them.
#[cfg(any(test, feature = "testing"))]
pub fn the_retry_waits_are(waits: Waits, cx: &mut gpui_kit::App) {
    cx.set_global(RetryWaits(waits));
}
