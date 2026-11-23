//! What a subscription that follows an issue or a pull request is sent first.

use protocol::client::ApiError;
use protocol::client::listed::ReferenceObservations;
use protocol::model::{Level, ReferenceAsk};

use crate::hub::Hub;
use crate::store::Store;

impl Hub {
    /// The observations of `reference`, as a chip that follows it draws them, with the hub's newest revision.
    pub(crate) async fn reference_listed(&self, reference: &ReferenceAsk) -> Result<ReferenceObservations, ApiError> {
        let level = Level::Reference { reference: reference.clone() };
        let (observations, rev) = self.observations.listed_at(&level, self.blocking(Store::current_rev)).await?;
        Ok(ReferenceObservations { reference: reference.clone(), observations, rev })
    }
}
