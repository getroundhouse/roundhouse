//! What a view reads of an agent's list of models: what the agent's CLI last listed, rather than whether the hub is
//! asking it, which the hub decides. It's read here alone, from the list's record, so that where the hub sends it apart
//! from the record, as an observation, only this changes.

use protocol::capability::settings::ModelCatalog;
use protocol::model::ModelList;

use super::{Changes, Snapshot};

impl Snapshot {
    /// What `list`'s agent offers, as its CLI last listed it: the catalog of the list followed, whether this copy holds
    /// that list's record or a reply carried it. `None` until the CLI has listed it.
    pub fn model_catalog<'a>(&'a self, list: &'a ModelList) -> Option<&'a ModelCatalog> {
        list.catalog.as_ref()
    }
}

impl Changes<'_> {
    /// Whether any of the hub's lists of models, or what one offers, changed, came or went.
    pub fn model_lists(&self) -> bool {
        self.now.records.model_lists != self.then.records.model_lists
    }
}
