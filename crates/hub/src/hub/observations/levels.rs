//! What a subscription is listed of the observations of a level other than the hub's, which `Hub::records_list` lists
//! with the records.

use protocol::client::ApiError;
use protocol::client::listed::WorkspaceObservations;
use protocol::model::{Level, WorkspaceId};

use crate::hub::Hub;
use crate::store::Store;

impl Hub {
    /// Every observation of the workspace `id`, with the revision they're as of, for a subscription that follows it.
    pub(crate) async fn workspace_observations(&self, id: &WorkspaceId) -> Result<WorkspaceObservations, ApiError> {
        let level = Level::Workspace { id: id.clone() };
        let (observations, rev) = self.observations.listed_at(&level, self.blocking(Store::current_rev)).await?;
        Ok(WorkspaceObservations { workspace_id: id.clone(), observations, rev })
    }
}
