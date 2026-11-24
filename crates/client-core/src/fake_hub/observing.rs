//! The observations a fake hub sends, as a hub of version 5 sends them: those its world names, and one for each thing
//! the world's records say that a hub keeps as an observation, such as an account's plan or a chat's context, so a world
//! says it where it always has. One the world names replaces the one its records would make, as a world says that one
//! isn't known.

use std::collections::HashMap;

use protocol::model::{Level, Observation, ObservationKey, Records};

use super::world::World;
use crate::records::observed_of;

impl World {
    /// The world's observations sent at `level`: those it names, and those its records make that it names none of.
    pub(super) fn observations_at(&self, level: &Level) -> Vec<Observation> {
        let records = Records { github_reach: self.github_reach.clone(), ..self.records() };
        let mut kept: HashMap<ObservationKey, Observation> =
            observed_of(&records).into_iter().map(|observation| (observation.key(), observation)).collect();
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
