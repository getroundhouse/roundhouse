use super::super::Hub;
use crate::error::provider_failed;
use crate::store::RecordedInstance;
use protocol::client::ApiError;
use protocol::dialing::WorkerTicket;
use protocol::model::InstanceId;
use providers::Provider;
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

    /// As the hub starts, admits the workers that dial it from then on, if any instance has a worker that dials, or a
    /// ticket one could dial with, which may be one an earlier hub left.
    pub(crate) async fn listen_for_workers(&self) {
        let store = self.store.clone();
        let dialing = tokio::task::spawn_blocking(move || store.has_dialing_workers()).await;
        match dialing {
            Ok(Ok(false)) => {}
            Ok(Ok(true)) => {
                if let Err(e) = self.dial.listen().await {
                    warn!(name: "dialed.unbound", error = e.message, "Workers can't dial the hub");
                }
            }
            Ok(Err(e)) => {
                warn!(name: "dialed.unread", error = %e, "Couldn't read which instances have workers that dial");
            }
            Err(e) => warn!(name: "dialed.unread", error = %e, "Couldn't read which instances have workers that dial"),
        }
    }

    /// A ticket for the instance's next worker, which its provider's `start` gives a worker it starts there.
    pub(crate) async fn worker_ticket(&self, id: &InstanceId) -> Result<WorkerTicket, ApiError> {
        // Boxed, since every future that can start a worker holds it, and binding an endpoint is a large one.
        Box::pin(self.dial.ticket(id)).await
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
