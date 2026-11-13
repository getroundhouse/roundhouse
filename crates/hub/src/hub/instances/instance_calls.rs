use super::super::Hub;
use crate::error::provider_failed;
use crate::store::RecordedInstance;
use protocol::client::ApiError;
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
                Ok(()) => info!(instance_id = %instance.id, "destroyed an instance the last run left part way"),
                Err(e) => {
                    let provider = &instance.provider;
                    warn!(instance_id = %instance.id, %provider, "an instance the last run left part way is still there: {e}");
                }
            }
        }
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
