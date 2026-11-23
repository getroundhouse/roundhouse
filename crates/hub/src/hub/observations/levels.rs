//! What a subscription is listed of the observations of a level other than the hub's, which `Hub::records_list` lists
//! with the records.

use protocol::client::ApiError;
use protocol::model::{Level, Observation, Rev};

use crate::hub::Hub;
use crate::store::Store;

impl Hub {
    /// Every observation of `level`, which isn't the hub's, such as a workspace's or a chip's, for a subscription that
    /// follows it, with the hub's newest revision, which covers no change the list lacks.
    pub(crate) async fn observations_at(&self, level: &Level) -> Result<(Vec<Observation>, Rev), ApiError> {
        self.observations.listed_at(level, self.blocking(Store::current_rev)).await
    }
}
