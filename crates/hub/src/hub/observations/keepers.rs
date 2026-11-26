//! What keeps an observation, and what a kind of observation is once nothing does, so whatever keeps several goes with
//! one call, as a worker or an agent's run goes, and no kind it kept is left out.

use protocol::model::RunId;

use super::kept::{Changed, Said, Step, Table};
use super::kinds::Kind;
use crate::worker_link::WorkerLinkId;

/// What keeps an observation, as the store keeps it of each, so whatever it keeps is let go of with it, as its kind
/// says, once it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keeper {
    /// A workspace's worker, by its link, as it watches its worktree or says which build it is: what it says is current
    /// while it does.
    Worker(WorkerLinkId),
    /// An agent's run, as it says what's only true of it, such as its tool servers: what it says is as of when it said
    /// it, never current, though the run goes on, as `docs/principles/keeping-observations.md` says of what an agent
    /// says outside its log.
    Run(RunId),
    /// The hub itself, reading it again while it's wanted, as it does what GitHub says of what a client follows: what it
    /// reads is current while it does.
    Hub,
}

impl Keeper {
    /// Whether what it says is current while it keeps it.
    pub(super) fn keeps_current(self) -> bool {
        match self {
            Keeper::Worker(_) | Keeper::Hub => true,
            Keeper::Run(_) => false,
        }
    }
}

/// What an observation is once what kept it stops, which each kind says, as [`Kind::UNKEPT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unkept {
    /// As of when it stopped, since what kept it would have said of any change until then, as a worker watching a
    /// worktree would.
    AsOfThen,
    /// As of its last read, since nothing said of a change since, as what the hub reads again only now and then.
    AsOfItsLastRead,
    /// Gone, since it was only ever true of what kept it, as which build a worker is, or which tool servers a run has.
    Gone,
}

impl Unkept {
    /// What it makes of an observation that nothing keeps from `at_ms`.
    pub(super) fn at(self, at_ms: i64) -> Unkeeping {
        match self {
            Unkept::AsOfThen => Unkeeping::AsOf(at_ms),
            Unkept::AsOfItsLastRead => Unkeeping::AsOfItsLastRead,
            Unkept::Gone => Unkeeping::Gone,
        }
    }
}

/// What an observation is from when nothing keeps it.
pub(super) enum Unkeeping {
    /// As of the time it holds, in ms by the hub's clock, or of its last read where that's later, as one that landed
    /// since can be.
    AsOf(i64),
    AsOfItsLastRead,
    Gone,
}

impl<K: Kind> Table<K> {
    /// What `keeper` keeping the observation of `of` makes of it. Another that kept it stops, so it's as its kind says
    /// once unkept, as of `at_ms`, and what was read before isn't current until the next read.
    pub(super) fn kept_by(&mut self, of: &K::Of, keeper: Keeper, at_ms: i64) -> Step<K::Value> {
        let before = self.keepers.insert(of.clone(), keeper);
        if before.is_none_or(|before| before == keeper) {
            return Step::Same;
        }
        self.unkept(of, K::UNKEPT.at(at_ms))
    }

    /// What the observation of `of` is once nothing keeps it, as `unkeeping` says. Leaves what keeps it to the caller.
    pub(super) fn unkept(&self, of: &K::Of, unkeeping: Unkeeping) -> Step<K::Value> {
        let Some(kept) = self.said(of) else { return Step::Same };
        match unkeeping {
            Unkeeping::Gone => Step::Changed(Changed::Gone),
            Unkeeping::AsOf(as_of_ms) if kept.current => {
                let at = as_of_ms.max(kept.read_at_ms);
                Step::Changed(Changed::Says(Said { current: false, as_of_ms: at, read_at_ms: at, ..kept.clone() }))
            }
            Unkeeping::AsOfItsLastRead if kept.current => {
                Step::Changed(Changed::Says(Said { current: false, as_of_ms: kept.read_at_ms, ..kept.clone() }))
            }
            Unkeeping::AsOf(_) | Unkeeping::AsOfItsLastRead => Step::Same,
        }
    }

    /// What each observation `keeper` keeps is of.
    pub(super) fn kept_by_keeper(&self, keeper: Keeper) -> Vec<K::Of> {
        self.keepers.iter().filter(|(_, by)| **by == keeper).map(|(of, _)| of.clone()).collect()
    }

    /// What each observation of a kind that's gone once unkept is of, that nothing keeps, as what was loaded of one is
    /// until what it was true of says it again. None of a kind every one of which the hub keeps.
    pub(super) fn unkept_of_a_kind_gone_once_unkept(&self) -> Vec<K::Of> {
        if K::UNKEPT != Unkept::Gone || self.all_kept_current {
            return Vec::new();
        }
        self.kept.keys().filter(|of| !self.keepers.contains_key(*of)).cloned().collect()
    }
}
