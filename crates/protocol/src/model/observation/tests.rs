use super::{Observation, ObservationKey, Observed};
use crate::model::{ChangeSummary, Level, LineCounts, Rev};

/// An observation of each kind, with the key and revision it's of, and the level it's sent at.
fn of_each_kind() -> Vec<(Observation, ObservationKey, Rev, Option<Level>)> {
    let lines = Observed::Current { value: LineCounts { added: 3, removed: 1 } };
    let changes = Observed::AsOf { value: ChangeSummary::new("c1".into(), Vec::new()), as_of_ms: 1_000 };
    vec![
        (
            Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(4) },
            ObservationKey::WorkspaceLines { workspace_id: "w".into() },
            Rev::new(4),
            Some(Level::Hub),
        ),
        (
            Observation::WorkspaceChanges { workspace_id: "w".into(), changes, rev: Rev::new(5) },
            ObservationKey::WorkspaceChanges { workspace_id: "w".into() },
            Rev::new(5),
            Some(Level::Workspace { id: "w".into() }),
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
