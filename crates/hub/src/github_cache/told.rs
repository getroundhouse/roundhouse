//! What the cache tells of what it keeps and forgets, for what's sent of it to change with it.

use tokio::sync::broadcast;

use super::kept::GithubCache;
use super::keys::{Facet, Item, Subject};

/// What a read kept, or a forgetting, changed of what the cache keeps.
#[derive(Clone, Debug, Default)]
pub(crate) struct Told {
    /// The issues and pull requests a read was of, whether or not what it found changed.
    pub(crate) kept: Vec<Item>,
    /// The issues and pull requests whose summaries the cache forgot, since nothing had asked for them in
    /// [`KEPT_FOR`](super::kept::KEPT_FOR).
    pub(crate) forgotten: Vec<Item>,
}

/// How many reads kept a subscriber can fall behind by before it's told it missed some, and sends everything again.
pub(super) const KEPT_TOLD: usize = 64;

impl GithubCache {
    /// Tells of each read kept from now on, by the issues and pull requests it was of, whether or not what it found
    /// changed, and of each summary forgotten.
    pub(crate) fn changes(&self) -> broadcast::Receiver<Told> {
        self.told.subscribe()
    }

    /// Tells of what a read kept, and of what was forgotten as it was kept, once both are, so what's told finds them.
    /// Nobody may be listening.
    pub(super) fn tell(&self, kept: Vec<Item>, forgotten: &[(Subject, Facet)]) {
        let forgotten: Vec<Item> = (forgotten.iter())
            .filter_map(|(subject, facet)| match (subject, facet) {
                (Subject::Item(item), Facet::Summary) => Some(item.clone()),
                _ => None,
            })
            .collect();
        if !kept.is_empty() || !forgotten.is_empty() {
            let _ = self.told.send(Told { kept, forgotten });
        }
    }
}
