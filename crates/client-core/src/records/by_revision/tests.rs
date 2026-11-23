use protocol::client::HubNotice;
use protocol::client::listed::WorkspaceObservations;
use protocol::model::{
    ChangeSummary, Level, LineCounts, Observation, ObservationKey, Observed, Records, Rev, WorkspaceId,
};

use super::super::{HubRecords, Taken};
use super::{ByRevision, Revised};

/// A kind made up for these tests, of which the hub's list has some, by their key, a workspace's list others, and no
/// list the rest, as with observations of each level.
#[derive(Clone, Debug, PartialEq)]
struct Made {
    key: MadeKey,
    value: u8,
    rev: Rev,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MadeKey {
    at: At,
    /// Which one of its kind it is.
    of: u8,
}

/// Which list has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum At {
    Hub,
    /// The workspace `w`'s.
    Workspace,
    None,
}

impl Revised for Made {
    type Key = MadeKey;

    fn key(&self) -> Option<MadeKey> {
        Some(self.key.clone())
    }

    fn rev(&self) -> Rev {
        self.rev
    }

    fn level(key: &MadeKey) -> Option<Level> {
        match key.at {
            At::Hub => Some(Level::Hub),
            At::Workspace => Some(w()),
            At::None => None,
        }
    }
}

const LISTED: MadeKey = MadeKey { at: At::Hub, of: 0 };
const APART: MadeKey = MadeKey { at: At::None, of: 1 };
const OF_W: MadeKey = MadeKey { at: At::Workspace, of: 2 };

fn w() -> Level {
    Level::Workspace { id: "w".into() }
}

fn made(key: MadeKey, value: u8, rev: u64) -> Made {
    Made { key, value, rev: Rev::new(rev) }
}

fn value(kept: &ByRevision<Made>, key: &MadeKey) -> Option<u8> {
    kept.held.get(key).map(|held| held.copy.value)
}

/// A list says nothing of what it doesn't have: not that one is gone, and not what an older copy says, so only a newer
/// copy, or its removal, replaces the one held.
#[test]
fn one_no_list_has_outlasts_every_list_but_goes_when_removed() {
    let (mut kept, mut taken) = (ByRevision::<Made>::default(), Taken::default());
    kept.apply_list(&Level::Hub, Rev::new(5), [made(LISTED, 1, 4)], &mut taken);

    assert!(kept.apply(made(APART, 1, 3), &mut taken), "older than the list, which doesn't have it");
    assert!(!kept.apply(made(LISTED, 2, 3), &mut taken), "older than the list, which has it");
    kept.apply_list(&Level::Hub, Rev::new(7), [], &mut taken);
    assert_eq!((value(&kept, &APART), value(&kept, &LISTED)), (Some(1), None), "a list without either");
    assert!(kept.remove(&APART, Rev::new(6)), "a removal older than the list, which doesn't have it");
    kept.apply_list(&Level::Hub, Rev::new(8), [], &mut taken);
    assert!(!kept.apply(made(APART, 2, 5), &mut taken), "an older copy after a list, which kept the removal");

    assert_eq!(value(&kept, &APART), None);
}

/// Every level keeps one rule: its list drops what it lacks of that level alone, a copy or a removal of that level no
/// newer than its newest list is already said, and so is a list older than one taken in, while a copy of another level
/// goes by that level's list. Letting go of a level forgets how far its list went.
#[test]
fn a_level_s_list_says_what_any_copy_or_list_of_it_no_newer_says() {
    let (mut kept, mut taken) = (ByRevision::<Made>::default(), Taken::default());
    let also_of_w = MadeKey { at: At::Workspace, of: 3 };
    kept.apply_list(&Level::Hub, Rev::new(9), [made(LISTED, 1, 9)], &mut taken);
    assert!(kept.apply_list(&w(), Rev::new(5), [made(also_of_w.clone(), 1, 3)], &mut taken), "w's level listed");

    assert!(!kept.apply(made(OF_W, 1, 4), &mut taken), "w's, from before w's list, which lacks it");
    assert!(!kept.remove(&OF_W, Rev::new(5)), "a removal w's list already says");
    assert!(kept.apply(made(OF_W, 2, 6), &mut taken), "w's, after its list, though the hub's list is newer");
    assert!(!kept.apply_list(&w(), Rev::new(4), [], &mut taken), "a list of w older than the one taken in");
    assert_eq!((value(&kept, &OF_W), value(&kept, &also_of_w)), (Some(2), Some(1)));
    assert!(kept.let_go(&w()));
    assert!(kept.apply(made(OF_W, 3, 4), &mut taken), "w's list was let go of with what's held of it");

    assert_eq!((value(&kept, &OF_W), value(&kept, &LISTED)), (Some(3), Some(1)));
}

/// A client that stops following a level lets go of what it held of that level, copies and removals alike, since
/// nothing tells it of them while it doesn't follow it, and what it holds of every other level stays.
#[test]
fn letting_go_of_a_level_drops_its_copies_and_removals_and_nothing_else() {
    let (mut kept, mut taken) = (ByRevision::<Made>::default(), Taken::default());
    let other = MadeKey { at: At::Workspace, of: 3 };
    kept.apply(made(OF_W, 1, 3), &mut taken);
    kept.apply(made(LISTED, 1, 3), &mut taken);
    kept.apply(made(APART, 1, 3), &mut taken);
    kept.remove(&other, Rev::new(6));

    assert!(kept.let_go(&w()));

    assert_eq!((value(&kept, &OF_W), value(&kept, &LISTED), value(&kept, &APART)), (None, Some(1), Some(1)));
    assert!(kept.apply(made(other, 3, 4), &mut taken), "its removal was let go of with it");
    assert!(!kept.let_go(&Level::Workspace { id: "v".into() }), "nothing held of that");
}

/// Letting go of a workspace's level leaves what's sent at the hub's level, such as a workspace's lines added and
/// removed, which every client that follows the hub is sent.
#[test]
fn letting_go_of_a_workspaces_level_leaves_its_observations_of_the_hubs_level() {
    let mut held = HubRecords::default();
    held.apply_list(list(vec![lines("w", 1, 2)], 3));

    let changed = held.let_go_of(&Level::Workspace { id: "w".into() });

    assert_eq!((changed, shown(&held, "w")), (false, current(1)));
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

fn changes(workspace: &str, base: &str, rev: u64) -> Observation {
    let changes = Observed::AsOf { value: ChangeSummary::new(base.into(), Vec::new()), as_of_ms: 1 };
    Observation::WorkspaceChanges { workspace_id: workspace.into(), changes, rev: Rev::new(rev) }
}

fn workspace_list(workspace: &str, observations: Vec<Observation>, rev: u64) -> WorkspaceObservations {
    WorkspaceObservations { workspace_id: workspace.into(), observations, rev: Rev::new(rev) }
}

/// What a view reads of a workspace's changed files.
#[derive(Debug, PartialEq)]
enum Read {
    /// That the hub hasn't listed the workspace's level.
    Unsent,
    NotKnown,
    /// The files, against this base.
    Against(String),
}

fn read(held: &HubRecords, workspace: &str) -> Read {
    match held.snapshot().workspace_changes(&WorkspaceId::from(workspace)) {
        None => Read::Unsent,
        Some(Observed::AsOf { value, .. } | Observed::Current { value }) => {
            Read::Against(value.base_commit.to_string())
        }
        Some(Observed::NotKnown | Observed::Unknown) => Read::NotKnown,
    }
}

/// A workspace's own list says what it holds of that workspace's level alone: a copy of it the list lacks is gone, and
/// those of another workspace, and of the hub's level, stay. Until a workspace's list has come its changed files read
/// as the hub not sending them, and once it has, as not known until the hub has read them. Letting go of the level
/// takes that back, so following it again waits for a list.
#[test]
fn a_workspace_s_list_says_what_is_held_of_its_level_alone_until_it_s_let_go_of() {
    let mut held = HubRecords::default();
    assert!(held.apply_notice(HubNotice::Observation(changes("v", "b1", 2))));
    held.apply_list(list(vec![lines("w", 1, 2)], 3));
    assert_eq!(read(&held, "w"), Read::Unsent, "no list of w's level yet");

    assert!(held.apply_workspace_list(workspace_list("w", Vec::new(), 4)), "the hub sends w's level");
    assert_eq!(read(&held, "w"), Read::NotKnown, "not read yet");
    assert!(held.apply_notice(HubNotice::Observation(changes("w", "b1", 5))));
    let before = held.snapshot();
    assert!(held.apply_workspace_list(workspace_list("w", vec![changes("w", "b2", 6)], 6)));
    assert!(held.snapshot().changes_since(&before).workspace_changes(&"w".into()));
    assert!(!held.snapshot().changes_since(&before).workspace_changes(&"v".into()));
    let lines_of_w = |snapshot: &super::super::Snapshot| std::ptr::from_ref(snapshot.workspace_lines(&"w".into()));
    assert_eq!(lines_of_w(&held.snapshot()), lines_of_w(&before), "w's lines, which didn't change, are shared");
    assert_eq!(read(&held, "w"), Read::Against("b2".into()));
    assert!(held.apply_workspace_list(workspace_list("w", Vec::new(), 7)), "gone from the list");
    held.apply_workspace_list(workspace_list("v", vec![changes("v", "b1", 2)], 8));

    assert_eq!((read(&held, "w"), read(&held, "v")), (Read::NotKnown, Read::Against("b1".into())));
    assert_eq!(shown(&held, "w"), current(1), "the hub's level stays");
    assert!(held.let_go_of(&Level::Workspace { id: "w".into() }));
    assert_eq!((read(&held, "w"), read(&held, "v")), (Read::Unsent, Read::Against("b1".into())));
}
