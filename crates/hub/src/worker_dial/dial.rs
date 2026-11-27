use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dialing::{HubEndpoint, Relays};
use protocol::client::{ApiError, ErrorCode};
use protocol::dialing::{Ticket, TicketSecret, WorkerTicket};
use protocol::model::InstanceId;
use protocol::paths::{LOOPBACK_WORKER_ENDPOINT_FILE, WORKER_ENDPOINT_FILE};
use providers::WorkerReach;
use tokio::sync::OnceCell;

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
}

/// One of the hub's endpoints, and where its key is kept.
struct OneEndpoint {
    key_file: PathBuf,
    relays: Relays,
    bound: OnceCell<Arc<HubEndpoint>>,
}

impl WorkerDial {
    /// With the keys in `dir`, each made the first time its endpoint binds, and the relayed one on `relays`.
    pub(crate) fn new(dir: &Path, relays: Relays, store: Arc<Store>) -> Self {
        let one = |file: &str, relays| OneEndpoint { key_file: dir.join(file), relays, bound: OnceCell::new() };
        Self {
            loopback: one(LOOPBACK_WORKER_ENDPOINT_FILE, Relays::None),
            relayed: one(WORKER_ENDPOINT_FILE, relays),
            store,
            arrivals: Arc::default(),
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
                let bound = Arc::new(HubEndpoint::bind(&one.key_file, one.relays).await?);
                tokio::spawn(super::admitting::admit(bound.clone(), self.store.clone(), self.arrivals.clone()));
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

    /// Closes every worker's connection, as the hub stops, telling each so.
    pub(crate) async fn close(&self) {
        for one in [&self.loopback, &self.relayed] {
            if let Some(bound) = one.bound.get() {
                bound.close().await;
            }
        }
    }
}
