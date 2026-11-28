use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dialing::{HubEndpoint, LOOPBACK_PORTS, Ports, Relays};
use protocol::client::{ApiError, ErrorCode};
use protocol::dialing::{Ticket, TicketSecret, WorkerTicket};
use protocol::model::InstanceId;
use protocol::paths::{LOOPBACK_WORKER_ADDRESS_FILE, LOOPBACK_WORKER_ENDPOINT_FILE, WORKER_ENDPOINT_FILE};
use providers::WorkerReach;
use tokio::sync::OnceCell;
use tracing::warn;

use super::Dialed;
use super::arrivals::Arrivals;
use crate::remote::random_text;
use crate::store::Store;

/// Which of the hub's endpoints a worker dials.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Endpoint {
    /// The one on loopback alone, with no relay and no port mapping, which the workers on the hub's own machine dial.
    /// So a hub whose workers are all there reaches nothing beyond loopback.
    Loopback,
    /// The one on the hub's relays, which a worker on any machine can reach.
    Relayed,
}

impl Endpoint {
    /// The one a provider's workers dial, as `reach` says, or `None` for workers that don't.
    pub(crate) fn dialed_by(reach: WorkerReach) -> Option<Self> {
        match reach {
            WorkerReach::Loopback => Some(Self::Loopback),
            WorkerReach::Relayed => Some(Self::Relayed),
            WorkerReach::Command => None,
        }
    }
}

/// Which UDP ports each of the hub's endpoints binds, where it has no relays, as the loopback one never has, and a
/// test's relayed one hasn't either. Each has at least one, as [`Ports`] holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointPorts {
    pub loopback: Ports,
    pub relayed: Ports,
}

impl EndpointPorts {
    /// What a hub binds: both from [`LOOPBACK_PORTS`].
    pub const PRODUCT: Self = Self { loopback: LOOPBACK_PORTS, relayed: LOOPBACK_PORTS };

    /// How many ports a test's hub is given, as [`EndpointPorts::for_a_test`] says.
    #[cfg(any(test, feature = "testing"))]
    pub const FOR_A_TEST: u16 = 4;

    /// A test's hub's, the [`EndpointPorts::FOR_A_TEST`] ports from `first`, two for each endpoint, which no other hub
    /// of the same run is given, since the run hands each its own. Drawn from the same range as other hubs, a test's
    /// could take the port another test's hub kept while it restarts, and an adapter's worker, which isn't given the
    /// loopback endpoint's address file and has no relay in a test, would then never find that hub again. Two each, so
    /// a hub binds the other if one is taken by something outside the run. `None` if they'd run past the last port.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn for_a_test(first: u16) -> Option<Self> {
        let half = Self::FOR_A_TEST / 2;
        let (middle, end) = (first.checked_add(half)?, first.checked_add(Self::FOR_A_TEST)?);
        Some(Self { loopback: Ports::new(first..middle)?, relayed: Ports::new(middle..end)? })
    }
}

/// The hub's two endpoints, each bound the first time it's wanted, so a hub that has made no ticket for a worker on
/// another machine never reaches a relay, and one whose workers all run elsewhere binds no loopback one. Binding one
/// leaves the other's connections as they are. Both admit workers to the same instances, by the same tickets and keys,
/// and the workers they've admitted wait in one place.
pub(crate) struct WorkerDial {
    loopback: OneEndpoint,
    relayed: OneEndpoint,
    /// Whose instances' records say which worker to admit.
    store: Arc<Store>,
    arrivals: Arc<Arrivals>,
    /// Set once the hub is letting go of its workers, as it stops, from when it refuses every worker that dials.
    refusing: Arc<AtomicBool>,
}

/// One of the hub's endpoints, and where its key is kept.
struct OneEndpoint {
    key_file: PathBuf,
    relays: Relays,
    ports: Ports,
    /// Where it writes its address each time it binds, for the loopback one, as `dialing::address_file` says.
    address_file: Option<PathBuf>,
    bound: OnceCell<Arc<HubEndpoint>>,
}

impl WorkerDial {
    /// With the keys in `dir`, each made the first time its endpoint binds, and the loopback one's address there too,
    /// the relayed one on `relays`, and each on `ports` where it has no relays.
    pub(crate) fn new(dir: &Path, relays: Relays, ports: EndpointPorts, store: Arc<Store>) -> Self {
        let one = |file: &str, relays, ports, address_file: Option<&str>| OneEndpoint {
            key_file: dir.join(file),
            relays,
            ports,
            address_file: address_file.map(|file| dir.join(file)),
            bound: OnceCell::new(),
        };
        Self {
            loopback: one(
                LOOPBACK_WORKER_ENDPOINT_FILE,
                Relays::None,
                ports.loopback,
                Some(LOOPBACK_WORKER_ADDRESS_FILE),
            ),
            relayed: one(WORKER_ENDPOINT_FILE, relays, ports.relayed, None),
            store,
            arrivals: Arc::default(),
            refusing: Arc::default(),
        }
    }

    fn one(&self, endpoint: Endpoint) -> &OneEndpoint {
        match endpoint {
            Endpoint::Loopback => &self.loopback,
            Endpoint::Relayed => &self.relayed,
        }
    }

    /// Binds `endpoint`, if it isn't yet, and from then on admits the workers that dial it, until it's closed.
    pub(crate) async fn listen(&self, endpoint: Endpoint) -> Result<&Arc<HubEndpoint>, ApiError> {
        let one = self.one(endpoint);
        one.bound
            .get_or_try_init(|| async {
                let bound = Arc::new(HubEndpoint::bind(&one.key_file, one.relays, one.ports.clone()).await?);
                if let Some(file) = &one.address_file {
                    // A worker that can't read it still dials where its ticket says, as it did before.
                    if let Err(e) = dialing::address_file::write(file, &bound.address().await) {
                        warn!(name: "endpoint.address_unwritten", error = %e, "Couldn't write where the hub's loopback endpoint is");
                    }
                }
                let (store, arrivals, refusing) = (self.store.clone(), self.arrivals.clone(), self.refusing.clone());
                tokio::spawn(super::admitting::admit(bound.clone(), store, arrivals, refusing));
                Ok::<_, dialing::DialError>(bound)
            })
            .await
            .map_err(|e| ApiError::new(ErrorCode::Unavailable, format!("the hub's endpoint for workers: {e}")))
    }

    /// Whether `endpoint` is bound.
    #[cfg(test)]
    pub(crate) fn is_bound(&self, endpoint: Endpoint) -> bool {
        self.one(endpoint).bound.initialized()
    }

    /// A ticket for the instance's next worker, which dials `endpoint`, recorded on its record first, so the hub admits
    /// a worker that dials with it, after a restart too, and none with any earlier ticket of the instance's that went
    /// unused.
    pub(crate) async fn ticket(&self, instance_id: &InstanceId, endpoint: Endpoint) -> Result<WorkerTicket, ApiError> {
        let hub = self.listen(endpoint).await?.address().await;
        let secret = TicketSecret::from(random_text::<32>()?);
        let (store, id, recorded) = (self.store.clone(), instance_id.clone(), secret.clone());
        let recorded = tokio::task::spawn_blocking(move || store.instance_ticket(&id, &recorded))
            .await
            .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))??;
        if !recorded {
            return Err(ApiError::new(ErrorCode::NotFound, format!("the hub has no record of instance {instance_id}")));
        }
        Ok(WorkerTicket::from(&Ticket { hub, secret }))
    }

    /// The instance's worker, once it has dialed and been admitted, if it does within `within`.
    pub(crate) async fn worker(&self, instance_id: &InstanceId, within: Duration) -> Option<Dialed> {
        self.arrivals.take(instance_id, within).await
    }

    /// Refuses every worker that dials from now on, as the hub stops, before it leaves or stops its workers. A worker
    /// the hub leaves for the next dials again as soon as its connection ends, and one admitted then would be cut off
    /// as the hub goes, and dial the next hub only after that. Refused, it dials until the next hub is there.
    pub(crate) fn refuse_more(&self) {
        self.refusing.store(true, Ordering::SeqCst);
    }

    /// Closes every worker's connection, as the hub stops, telling each so.
    pub(crate) async fn close(&self) {
        for one in [&self.loopback, &self.relayed] {
            if let Some(bound) = one.bound.get() {
                bound.close().await;
            }
        }
    }
}
