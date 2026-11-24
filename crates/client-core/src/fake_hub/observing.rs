//! The observations a fake hub sends, as a hub sends them: those its world names, and a summary of each workspace's
//! changes where the world has git's patch of them and names none.

use std::collections::HashMap;

use protocol::model::{Level, Observation, ObservationKey};

use super::world::World;

impl World {
    /// Has the hub hold `observation`, in place of any it held of the same thing.
    pub fn observe(&mut self, observation: Observation) {
        let key = observation.key();
        self.observations.retain(|held| held.key() != key);
        self.observations.push(observation);
    }

    /// The observation `key` the world names, if it names one.
    pub fn observed(&self, key: &ObservationKey) -> Option<&Observation> {
        self.observations.iter().rev().find(|observation| observation.key() == *key)
    }

    /// The world's observations sent at `level`: those it names, and the summary of each workspace's changes it has a
    /// patch of and names none of.
    pub(super) fn observations_at(&self, level: &Level) -> Vec<Observation> {
        let mut kept: HashMap<ObservationKey, Observation> = HashMap::new();
        // A workspace's changed files, as a worker reads them, where the world has git's patch of them.
        for workspace in self.diffs.keys() {
            kept.extend(self.summary_of_diff(workspace).map(|summary| (summary.key(), summary)));
        }
        for named in &self.observations {
            kept.insert(named.key(), named.clone());
        }
        let mut at_level: Vec<Observation> =
            kept.into_values().filter(|observation| observation.key().level().as_ref() == Some(level)).collect();
        at_level.sort_by_key(|observation| format!("{:?}", observation.key()));
        at_level
    }
}
