//! Which chips each view shows. What the hub sends of them, as observations, is held with every other observation, in
//! the copy of the hub's records, which views read through [`crate::Snapshot`].

use std::collections::BTreeMap;

use protocol::client::github::MOST_REFERENCES_SHOWN;
use protocol::model::ReferenceAsk;

/// Which chips each view shows.
#[derive(Default)]
pub(super) struct GithubHeld {
    /// By ID, which is in the order they were made, so all they show is said in the same order each time.
    views: BTreeMap<u64, Vec<ReferenceAsk>>,
}

impl GithubHeld {
    /// Adds the view `view`, which shows nothing yet.
    pub(super) fn add(&mut self, view: u64) {
        self.views.insert(view, Vec::new());
    }

    /// Takes `chips` to be what the view `view` shows.
    pub(super) fn show(&mut self, view: u64, chips: Vec<ReferenceAsk>) {
        if let Some(shown) = self.views.get_mut(&view) {
            *shown = chips;
        }
    }

    pub(super) fn hide(&mut self, view: u64) {
        self.views.remove(&view);
    }

    /// Every chip every view shows, each once, but for those past the most a hub takes, which are those of the views
    /// made longest ago, since the newest is likeliest to be what's in front.
    pub(super) fn shown(&self) -> Vec<ReferenceAsk> {
        let mut all = Vec::new();
        for chips in self.views.values().rev() {
            for chip in chips {
                if !all.contains(chip) {
                    all.push(chip.clone());
                }
            }
        }
        all.truncate(MOST_REFERENCES_SHOWN);
        all
    }
}
