use super::super::Hub;
use crate::error::provider_failed;
use crate::store::{RecordedInstance, Store};
use crate::worker_dial::Endpoint;
use protocol::client::ApiError;
use protocol::dialing::WorkerTicket;
use protocol::model::InstanceId;
use providers::Provider;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{info, warn};

impl Hub {
    /// For once the hub is serving, so a slow adapter can't keep clients waiting for it. Says hello to each provider's
    /// adapter, so a problem with one is logged before anything needs it, then destroys the instances in `unfinished`,
    /// which the last run left part way. One it can't destroy is left recorded, for the next start.
    pub(crate) async fn settle_providers(self: Arc<Self>, unfinished: Vec<RecordedInstance>) {
        self.providers.say_hello().await;
        for instance in unfinished {
            let destroyed = match self.provider(&instance.provider) {
                Ok(provider) => self.destroy_instance(provider.as_ref(), &instance.id).await,
                Err(e) => Err(e),
            };
            match destroyed {
                Ok(()) => {
                    info!(
                        name: "instance.destroyed",
                        instance_id = %instance.id,
                        "Destroyed an instance the last run left part way"
                    );
                }
                Err(e) => {
                    let provider = &instance.provider;
                    warn!(
                        name: "instance.left",
                        instance_id = %instance.id,
                        %provider,
                        error = %e,
                        "An instance the last run left part way is still there"
                    );
                }
            }
        }
    }

    /// As the hub starts, admits the workers that dial it from then on, at each endpoint the providers' workers dial
    /// whose instances have a worker that dials, or a ticket one could dial with, which may be one an earlier hub left.
    pub(crate) async fn listen_for_workers(&self) {
        let providers = match self.blocking(Store::providers_with_dialing_workers).await {
            Ok(providers) => providers,
            Err(e) => {
                warn!(name: "dialed.unread", error = e.message, "Couldn't read which instances have workers that dial");
                return;
            }
        };
        let endpoints: HashSet<Endpoint> = providers
            .iter()
            .filter_map(|name| self.provider(name).ok())
            .filter_map(|provider| Endpoint::dialed_by(provider.workers_reach()))
            .collect();
        for endpoint in endpoints {
            if let Err(e) = self.dial.listen(endpoint).await {
                warn!(name: "dialed.unbound", ?endpoint, error = e.message, "Workers can't dial the hub");
            }
        }
    }

    /// A ticket for the instance's next worker, which its provider's `start` gives a worker it starts there, to dial
    /// `endpoint` with.
    pub(crate) async fn worker_ticket(&self, id: &InstanceId, endpoint: Endpoint) -> Result<WorkerTicket, ApiError> {
        self.dial.ticket(id, endpoint).await
    }

    /// Destroys an instance the hub recorded. It's marked as being destroyed first, so a destroy cut short is finished
    /// when the hub next starts, and its record goes once its provider says it's gone.
    pub(crate) async fn destroy_instance(&self, provider: &dyn Provider, id: &InstanceId) -> Result<(), ApiError> {
        let owned = id.clone();
        self.blocking(move |store| store.instance_destroying(&owned)).await?;
        provider.destroy(id).await.map_err(|e| provider_failed(&e))?;
        let owned = id.clone();
        self.blocking(move |store| store.instance_destroyed(&owned)).await
    }
}
