//! What a view reads of an agent's list of models: what the agent's CLI last listed, which is an observation, beside
//! whether the hub is asking it, which is the list's record.

use protocol::capability::settings::ModelCatalog;
use protocol::model::{ModelListKey, ModelLookup, Observation, ObservationKey};

use super::observed::said;
use super::{Changes, Snapshot};

/// One of the hub's lists of models, as a view reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListOfModels<'a> {
    /// What the agent offers, as its CLI last listed it. `None` until the hub has said, as while the lookup has
    /// answered and what it found hasn't come.
    pub catalog: Option<&'a ModelCatalog>,
    /// Whether the hub is asking the agent, and how it went the last time.
    pub lookup: &'a ModelLookup,
}

impl Snapshot {
    /// The list of models `key` names. `None` until the hub has sent its record, and once the hub has let go of it.
    pub fn list_of_models(&self, key: &ModelListKey) -> Option<ListOfModels<'_>> {
        let list = self.records.model_lists.get(key)?;
        let held = self.observation(&ObservationKey::ModelCatalog { list: key.clone() });
        let catalog = if let Some(Observation::ModelCatalog { catalog, .. }) = held { said(catalog) } else { None };
        Some(ListOfModels { catalog, lookup: &list.lookup })
    }
}

impl Changes<'_> {
    /// Whether any of the hub's lists of models, or what one offers, changed, came or went.
    pub fn model_lists(&self) -> bool {
        fn catalogs(snapshot: &Snapshot) -> Vec<&Observation> {
            let of = |key: &ObservationKey| matches!(key, ObservationKey::ModelCatalog { .. });
            let mut held: Vec<_> = snapshot.observations.all(of).collect();
            held.sort_by_key(|observation| observation.rev());
            held
        }
        self.now.records.model_lists != self.then.records.model_lists || catalogs(self.now) != catalogs(self.then)
    }
}
