//! What a view reads of an agent's list of models: what the agent's CLI last listed, beside whether the hub is asking
//! it, which the hub decides. Both are read here alone, from the list's record, so that where the hub sends what the
//! CLI listed apart from the record, as an observation, only this changes.

use protocol::capability::settings::ModelCatalog;
use protocol::model::{ModelListKey, ModelLookup};

use super::{Changes, Snapshot};

/// One of the hub's lists of models, as a view reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListOfModels<'a> {
    /// What the agent offers, as its CLI last listed it. `None` until it has.
    pub catalog: Option<&'a ModelCatalog>,
    /// Whether the hub is asking the agent, and how it went the last time.
    pub lookup: &'a ModelLookup,
}

impl Snapshot {
    /// The list of models `key` names. `None` until the hub has sent it, and once the hub has let go of it.
    pub fn list_of_models(&self, key: &ModelListKey) -> Option<ListOfModels<'_>> {
        let list = self.records.model_lists.get(key)?;
        Some(ListOfModels { catalog: list.catalog.as_ref(), lookup: &list.lookup })
    }
}

impl Changes<'_> {
    /// Whether any of the hub's lists of models, or what one offers, changed, came or went.
    pub fn model_lists(&self) -> bool {
        self.now.records.model_lists != self.then.records.model_lists
    }
}
