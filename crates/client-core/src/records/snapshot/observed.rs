//! What a view reads of the hub's observations. One the client doesn't hold reads as not known.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use protocol::model::{ChangeSummary, LineCounts, Observation, ObservationKey, Observed, WorkspaceId};

use super::super::Taken;
use super::{Changes, Snapshot};

/// Every observation held, as views read them, each shared with what client-core holds, since one can hold a lot, such
/// as a workspace's changed files, and every other is read again whenever one changes.
#[derive(Debug, Default)]
pub(in super::super) struct ObservationsListed {
    /// By which it is, with when it was taken in, so a view can tell it changed without comparing it.
    held: HashMap<ObservationKey, (Arc<Observation>, Taken)>,
    /// The workspaces whose own level the hub has listed since they were followed.
    workspaces_listed: HashSet<WorkspaceId>,
    /// Whether the hub's level has been listed, after which an observation of it the client lacks isn't known.
    hub_listed: bool,
}

/// What a view reads of an observation the client doesn't hold.
const NOT_KNOWN: Observed<LineCounts> = Observed::NotKnown;

/// The same, of a workspace's changed files, from a hub that sends its level.
const CHANGES_NOT_KNOWN: Observed<ChangeSummary> = Observed::NotKnown;

impl ObservationsListed {
    /// From every observation held, with when each was taken in, the workspaces whose own level has been listed, and
    /// whether the hub's has.
    pub(in super::super) fn of(
        held: impl Iterator<Item = (Arc<Observation>, Taken)>,
        workspaces_listed: HashSet<WorkspaceId>,
        hub_listed: bool,
    ) -> Self {
        let held = held.map(|(observation, taken)| (observation.key(), (observation, taken))).collect();
        ObservationsListed { held, workspaces_listed, hub_listed }
    }

    /// Those of a kind `of` picks out, in no order.
    pub(super) fn all(&self, of: impl Fn(&ObservationKey) -> bool) -> impl Iterator<Item = &Observation> {
        self.held.iter().filter(move |(key, _)| of(key)).map(|(_, (observation, _))| observation.as_ref())
    }

    /// When the observation `key` names was taken in, or `None` where it isn't held.
    fn taken(&self, key: &ObservationKey) -> Option<Taken> {
        self.held.get(key).map(|(_, taken)| *taken)
    }
}

impl Snapshot {
    /// The observation `key` names, if the client holds it.
    pub(crate) fn observation(&self, key: &ObservationKey) -> Option<&Observation> {
        self.observations.held.get(key).map(|(observation, _)| observation.as_ref())
    }

    /// Whether the hub has listed its own level, after which one of its observations the client lacks isn't known.
    pub(crate) fn hub_listed(&self) -> bool {
        self.observations.hub_listed
    }

    /// How many lines the workspace's changes add and remove against its base, in all. Not known until the hub has
    /// sent it.
    pub fn workspace_lines(&self, workspace: &WorkspaceId) -> &Observed<LineCounts> {
        let key = ObservationKey::WorkspaceLines { workspace_id: workspace.clone() };
        let Some(Observation::WorkspaceLines { lines, .. }) = self.observation(&key) else { return &NOT_KNOWN };
        lines
    }

    /// Every file the workspace changes against its base, as the hub's summary of them says. `None` where the hub
    /// hasn't listed the workspace's own level since it was followed, as for a workspace nothing on this connection
    /// follows, and until its list comes. Not known where the hub has read none.
    pub fn workspace_changes(&self, workspace: &WorkspaceId) -> Option<&Observed<ChangeSummary>> {
        if !self.observations.workspaces_listed.contains(workspace) {
            return None;
        }
        let key = ObservationKey::WorkspaceChanges { workspace_id: workspace.clone() };
        let Some(Observation::WorkspaceChanges { changes, .. }) = self.observation(&key) else {
            return Some(&CHANGES_NOT_KNOWN);
        };
        Some(changes)
    }
}

impl Changes<'_> {
    /// Whether the workspace's changed files changed, came or went, or whether the hub has listed its level did.
    pub fn workspace_changes(&self, workspace: &WorkspaceId) -> bool {
        let listed = |snapshot: &Snapshot| snapshot.observations.workspaces_listed.contains(workspace);
        let key = ObservationKey::WorkspaceChanges { workspace_id: workspace.clone() };
        listed(self.now) != listed(self.then) || self.observation(&key)
    }

    /// Whether the observation `key` names changed, came or went.
    pub(super) fn observation(&self, key: &ObservationKey) -> bool {
        self.now.observations.taken(key) != self.then.observations.taken(key)
    }
}

/// What `observed` says, if it's known, however fresh.
pub(super) fn said<T>(observed: &Observed<T>) -> Option<&T> {
    match observed {
        Observed::Current { value } | Observed::AsOf { value, .. } => Some(value),
        Observed::NotKnown | Observed::Unknown => None,
    }
}

/// What `read` makes of what `observed` says, as fresh as it is.
pub(super) fn read<'a, T, U>(observed: &'a Observed<T>, read: impl FnOnce(&'a T) -> U) -> Observed<U> {
    match observed {
        Observed::Current { value } => Observed::Current { value: read(value) },
        Observed::AsOf { value, as_of_ms } => Observed::AsOf { value: read(value), as_of_ms: *as_of_ms },
        Observed::NotKnown => Observed::NotKnown,
        Observed::Unknown => Observed::Unknown,
    }
}
