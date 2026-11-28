//! A hub installed as a service updating itself to a release, as a client asks: downloading its archive, placing it
//! beside the build it runs, and restarting, which the service starts the new build after.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::paths::SERVICE_UPDATE_DIR;
use tracing::info;
use update_feed::HubArchive;

use super::looking::{Source, Starting};
use super::placing::place;
use super::reading::fetch_archive;
use crate::hub::Hub;
use crate::hub::observations::ReleaseOffered;

impl Hub {
    /// Starts updating to `version`, as [`HubUpdateTo`] says, and answers once it has: the update itself goes on after,
    /// and the hub's record says how it's going.
    ///
    /// [`HubUpdateTo`]: protocol::client::admin::HubUpdateTo
    pub(crate) async fn update_to(self: &Arc<Self>, version: String) -> Result<(), ApiError> {
        let Some(source) = self.updates.source().cloned() else {
            let why = "this hub doesn't read the update feed, so it isn't updated from here. Only a release of a hub \
                installed as a service is";
            return Err(ApiError::new(ErrorCode::InvalidRequest, why));
        };
        let offered = self.observations.value_of::<ReleaseOffered>(&());
        let Starting::Starts { version, archive } =
            self.change_update(|found| found.start_updating(&version, offered.as_ref())).await?
        else {
            return Ok(());
        };
        info!(name: "update.started", from = source.running, to = %version, "Updating the hub, as a client asked");
        tokio::spawn(Arc::clone(self).update(source, version, archive));
        Ok(())
    }

    /// Downloads and places `archive`, `version`'s, and restarts, or says why it couldn't. What it downloaded goes
    /// either way, and what an update cut short left goes as the next hub starts.
    async fn update(self: Arc<Self>, source: Source, version: String, archive: HubArchive) {
        let downloads = self.updates.service_builds.join(SERVICE_UPDATE_DIR);
        let placed = tokio::task::spawn_blocking(move || {
            let _ = std::fs::remove_dir_all(&downloads);
            let fetched = fetch_archive(&source.feed, &archive, &downloads).map_err(|e| e.to_string());
            let placed = fetched
                .and_then(|fetched| place(&fetched, &version, &downloads.join(&version)).map_err(|e| e.to_string()));
            let _ = std::fs::remove_dir_all(&downloads);
            placed
        })
        .await
        .unwrap_or_else(|e| Err(format!("the update stopped: {e}")));
        // No request of its own is under way by now: its reply went as it started.
        match placed {
            Ok(()) => self.request_restart(0),
            Err(why) => {
                // Only the store failing to give a revision stops this, which leaves the record saying it's updating.
                let _ = self
                    .change_update(|found| {
                        found.failed(why);
                        Ok(())
                    })
                    .await;
            }
        }
    }
}
