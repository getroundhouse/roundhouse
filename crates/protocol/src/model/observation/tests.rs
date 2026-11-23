use super::{LineCounts, Observation, ObservationKey, Observed};
use crate::model::Rev;

/// An observation of each kind, with the key and revision it's of, and whether it's sent at the hub's level.
fn of_each_kind() -> Vec<(Observation, ObservationKey, Rev, bool)> {
    let lines = Observed::Current { value: LineCounts { added: 3, removed: 1 } };
    vec![
        (
            Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(4) },
            ObservationKey::WorkspaceLines { workspace_id: "w".into() },
            Rev::new(4),
            true,
        ),
        (Observation::Unknown, ObservationKey::Unknown, Rev::default(), false),
    ]
}

#[test]
fn an_observation_says_which_it_is_its_revision_and_whether_it_is_sent_at_the_hub_s_level() {
    for (observation, key, rev, at_hub_level) in of_each_kind() {
        assert_eq!((observation.key(), observation.rev()), (key.clone(), rev), "{observation:?}");
        assert_eq!(key.at_hub_level(), at_hub_level, "{key:?}");
    }
}
