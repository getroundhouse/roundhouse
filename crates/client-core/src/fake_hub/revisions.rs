//! The revisions a fake hub serves its world at, as a hub serves its records: each at the revision of its newest
//! change, and every list at the hub's newest revision. A world is made whole before it's served, so a record its test
//! gave no revision is at [`MADE`], as if the world were made in one change. Nothing it serves is then at no revision,
//! which only a hub that keeps none sends, and which a client takes in the order it comes, whatever else it holds.

use protocol::model::{Observation, Rev};

use super::world::World;

/// The revision of each record a world holds that its test gave none.
pub(super) const MADE: Rev = Rev::new(1);

impl World {
    /// The hub's newest revision, which every list it sends is at: that of its newest record or observation, once each
    /// record with none is at [`MADE`].
    pub fn rev(&mut self) -> Rev {
        let records = self.stamped().into_iter().map(|rev| *rev).max().unwrap_or(MADE);
        self.observations.iter().map(Observation::rev).fold(records, Rev::max)
    }

    /// A revision past every one the hub's lists are at, for a change a test hands client-core as the feed would send
    /// it, after the world is served, so no list it sends takes the change back.
    pub fn after(&mut self) -> Rev {
        Rev::new(self.rev().get() + 1)
    }

    /// A revision past the hub's lists, for a record it makes as it answers, as a hub records what a request starts.
    pub(super) fn after_listed(&self) -> Rev {
        Rev::new(self.listed_at.get() + 1)
    }

    /// Every record's revision, with each that had none at [`MADE`].
    pub(super) fn stamped(&mut self) -> Vec<&mut Rev> {
        let mut revs: Vec<&mut Rev> = Vec::new();
        revs.extend(self.repos.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.workspaces.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.sessions.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.subagents.values_mut().flatten().map(|record| &mut record.rev));
        revs.extend(self.proposals.values_mut().flatten().map(|record| &mut record.rev));
        revs.extend(self.replies_heard.values_mut().flatten().map(|record| &mut record.rev));
        revs.extend(self.comments.values_mut().flatten().map(|record| &mut record.rev));
        revs.extend(self.accounts.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.github_accounts.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.github_sign_ins.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.github_apps.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.terminals.iter_mut().map(|(record, _)| &mut record.rev));
        revs.extend(self.model_lists.iter_mut().map(|record| &mut record.rev));
        revs.extend(self.hub_update.iter_mut().map(|record| &mut record.rev));
        for rev in &mut revs {
            if **rev == Rev::default() {
                **rev = MADE;
            }
        }
        revs
    }
}
