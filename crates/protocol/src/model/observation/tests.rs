use super::{Observation, ObservationKey, Observed};
use crate::model::{Level, LineCounts, Rev};

/// An observation of each kind, with the key and revision it's of, and the level it's sent at.
fn of_each_kind() -> Vec<(Observation, ObservationKey, Rev, Option<Level>)> {
    let lines = Observed::Current { value: LineCounts { added: 3, removed: 1 } };
    vec![
        (
            Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(4) },
            ObservationKey::WorkspaceLines { workspace_id: "w".into() },
            Rev::new(4),
            Some(Level::Hub),
        ),
        (Observation::Unknown, ObservationKey::Unknown, Rev::default(), None),
    ]
}

#[test]
fn an_observation_says_which_it_is_its_revision_and_the_level_it_is_sent_at() {
    for (observation, key, rev, level) in of_each_kind() {
        assert_eq!((observation.key(), observation.rev()), (key.clone(), rev), "{observation:?}");
        assert_eq!(key.level(), level, "{key:?}");
    }
}
