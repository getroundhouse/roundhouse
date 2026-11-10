//! The hub's own settings: keeping them, carrying them out, and telling every client.

use protocol::client::{ApiError, HubNotice};
use protocol::model::{HubSettings, HubSettingsChange};

use super::Hub;

impl Hub {
    /// Has the hub do what its settings say. They're stored, so this is also how a hub that's starting picks up where
    /// the last one left off, with no client there to say.
    pub(crate) fn carry_out(&self, settings: &HubSettings) {
        self.awake.want(settings.keep_awake);
    }

    /// One change at a time, so what's stored, what the hub does and what clients hear all change in the same order,
    /// however two clients' changes arrive.
    pub(crate) async fn hub_configure(&self, change: HubSettingsChange) -> Result<HubSettings, ApiError> {
        let _in_order = self.configuring.lock().await;
        let settings = self.blocking(move |store| store.change_hub_settings(&change)).await?;
        self.carry_out(&settings);
        self.notify(HubNotice::HubSettings(settings.clone()));
        Ok(settings)
    }
}
