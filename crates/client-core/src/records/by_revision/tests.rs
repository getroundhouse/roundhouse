use protocol::client::HubNotice;
use protocol::model::{LineCounts, Observation, ObservationKey, Observed, Records, Rev};

use super::super::{HubRecords, Taken};
use super::{ByRevision, Revised};

/// A kind made up for these tests, of which a list of everything has some, by their key, and not others, as with
/// observations of the hub's level and of another.
#[derive(Clone, Debug, PartialEq)]
struct Made {
    key: MadeKey,
    value: u8,
    rev: Rev,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MadeKey {
    in_every_list: bool,
}

impl Revised for Made {
    type Key = MadeKey;

    fn key(&self) -> Option<MadeKey> {
        Some(self.key.clone())
    }

    fn rev(&self) -> Rev {
        self.rev
    }

    fn in_every_list(key: &MadeKey) -> bool {
        key.in_every_list
    }
}

const LISTED: MadeKey = MadeKey { in_every_list: true };
const APART: MadeKey = MadeKey { in_every_list: false };

fn made(key: MadeKey, value: u8, rev: u64) -> Made {
    Made { key, value, rev: Rev::new(rev) }
}

fn value(kept: &ByRevision<Made>, key: &MadeKey) -> Option<u8> {
    kept.held.get(key).map(|held| held.copy.value)
}

/// A list says nothing of a kind it doesn't have: not that one is gone, and not what an older copy says, so only a
/// newer copy, or its removal, replaces the one held.
#[test]
fn one_of_a_kind_no_list_has_outlasts_every_list_but_goes_when_removed() {
    let (mut kept, mut taken) = (ByRevision::<Made>::default(), Taken::default());
    kept.apply_list(Rev::new(5), [made(LISTED, 1, 4)], &mut taken);

    assert!(kept.apply(made(APART, 1, 3), Rev::new(5), &mut taken), "older than the list, which doesn't have it");
    assert!(!kept.apply(made(LISTED, 2, 3), Rev::new(5), &mut taken), "older than the list, which has it");
    kept.apply_list(Rev::new(7), [], &mut taken);
    assert_eq!((value(&kept, &APART), value(&kept, &LISTED)), (Some(1), None), "a list without either");
    assert!(kept.remove(&APART, Rev::new(6), Rev::new(7)), "a removal older than the list, which doesn't have it");
    kept.apply_list(Rev::new(8), [], &mut taken);
    assert!(
        !kept.apply(made(APART, 2, 5), Rev::new(8), &mut taken),
        "an older copy after a list, which kept the removal"
    );

    assert_eq!(value(&kept, &APART), None);
}

fn lines(workspace: &str, added: u64, rev: u64) -> Observation {
    let lines = Observed::Current { value: LineCounts { added, removed: 0 } };
    Observation::WorkspaceLines { workspace_id: workspace.into(), lines, rev: Rev::new(rev) }
}

fn shown(held: &HubRecords, workspace: &str) -> Observed<LineCounts> {
    held.snapshot().workspace_lines(&workspace.into()).clone()
}

fn current(added: u64) -> Observed<LineCounts> {
    Observed::Current { value: LineCounts { added, removed: 0 } }
}

fn list(observations: Vec<Observation>, rev: u64) -> Records {
    Records { observations, rev: Rev::new(rev), ..Records::default() }
}

/// As every kind of record does, by the same rule, through a list, the feed's copies and its removal, and lists from
/// before and after a change. One the client doesn't hold reads as not known.
#[test]
fn an_observation_keeps_its_newest_copy_and_goes_when_the_feed_says_as_a_record_does() {
    let mut held = HubRecords::default();
    assert_eq!(shown(&held, "w"), Observed::NotKnown, "until it's arrived");
    held.apply_list(list(vec![lines("w", 1, 2), lines("v", 1, 2), Observation::Unknown], 3));
    let listed = held.snapshot();

    assert!(held.apply_notice(HubNotice::Observation(lines("w", 3, 5))), "a newer copy");
    assert!(!held.apply_notice(HubNotice::Observation(lines("w", 2, 4))), "an older copy that arrives late");
    assert!(!held.apply_notice(HubNotice::Observation(lines("v", 9, 1))), "a copy older than the list");
    assert!(!held.apply_notice(HubNotice::Observation(Observation::Unknown)), "a kind this build doesn't know");
    assert!(held.snapshot().changes_since(&listed).any());
    held.apply_list(list(vec![lines("w", 1, 2), lines("v", 1, 2)], 4));
    assert_eq!((shown(&held, "w"), shown(&held, "v")), (current(3), current(1)), "after a list from before the change");
    let gone = ObservationKey::WorkspaceLines { workspace_id: "w".into() };
    assert!(held.apply_notice(HubNotice::ObservationRemoved { observation: gone, rev: Rev::new(6) }));
    assert!(!held.apply_notice(HubNotice::Observation(lines("w", 3, 5))), "a copy older than its removal");
    held.apply_list(list(Vec::new(), 7));

    assert_eq!((shown(&held, "w"), shown(&held, "v")), (Observed::NotKnown, Observed::NotKnown));
}
