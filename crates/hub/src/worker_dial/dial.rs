use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dialing::{HubEndpoint, Relays};
use protocol::client::{ApiError, ErrorCode};
use protocol::dialing::{Ticket, TicketSecret, WorkerTicket};
use protocol::model::InstanceId;
use tokio::sync::OnceCell;

use super::Dialed;
use super::arrivals::Arrivals;
use crate::remote::random_text;
use crate::store::Store;

/// The hub's endpoint, bound the first time it's wanted, so a hub whose workers all run on its own machine binds none,
/// and the workers it has admitted.
pub(crate) struct WorkerDial {
    key_file: PathBuf,
    relays: Relays,
    /// Whose instances' records say which worker to admit.
    store: Arc<Store>,
    endpoint: OnceCell<Arc<HubEndpoint>>,
    arrivals: Arc<Arrivals>,
}

impl WorkerDial {
    /// With its key at `key_file`, made the first time it binds.
    pub(crate) fn new(key_file: PathBuf, relays: Relays, store: Arc<Store>) -> Self {
        Self { key_file, relays, store, endpoint: OnceCell::new(), arrivals: Arc::default() }
    }

    /// Binds the endpoint, if it isn't yet, and from then on admits the workers that dial it, until it's closed.
    pub(crate) async fn listen(&self) -> Result<&Arc<HubEndpoint>, ApiError> {
        self.endpoint
            .get_or_try_init(|| async {
                let endpoint = Arc::new(HubEndpoint::bind(&self.key_file, self.relays).await?);
                tokio::spawn(super::admitting::admit(endpoint.clone(), self.store.clone(), self.arrivals.clone()));
                Ok::<_, dialing::DialError>(endpoint)
            })
            .await
            .map_err(|e| ApiError::new(ErrorCode::Unavailable, format!("the hub's endpoint for workers: {e}")))
    }

    /// A ticket for the instance's next worker, recorded on its record first, so the hub admits a worker that dials
    /// with it, after a restart too, and none with any earlier ticket of the instance's that went unused.
    pub(crate) async fn ticket(&self, instance_id: &InstanceId) -> Result<WorkerTicket, ApiError> {
        let hub = self.listen().await?.address().await;
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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the hub reaches a dialed worker through it in the next PR of #2478's stack")
    )]
    pub(crate) async fn worker(&self, instance_id: &InstanceId, within: Duration) -> Option<Dialed> {
        self.arrivals.take(instance_id, within).await
    }

    /// Closes every worker's connection, as the hub stops, telling each so.
    pub(crate) async fn close(&self) {
        if let Some(endpoint) = self.endpoint.get() {
            endpoint.close().await;
        }
    }
}
