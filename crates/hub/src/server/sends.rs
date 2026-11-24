//! What each subscription is sent of the feed's notices, decided in one place, [`Sends::notice`], for `FeedSubscribe`
//! and for every level `Subscribe` follows: the notices of its level, in the forms its client reads. Which those are, a
//! copy's form from before observations or its observation, as `docs/principles/keeping-observations.md` has them, one
//! function decides, [`Sends::keeps`], by what each kind's [`ObservationKey::read_from`] says, the version of the
//! client protocol the subscription's connection chose, and whether its start reads GitHub as observations, whatever
//! request started the subscription. So what decides which clients still read an old form changes in one place.

use protocol::client::listed::{Listed, ReferenceObservations, SessionRecords, WorkspaceObservations};
use protocol::client::{EVERY_OBSERVATION_FROM, HubNotice};
use protocol::model::{Level, Observation, ObservationKey, ReadFrom, Records};

use crate::hub::{Fed, Moves};
use crate::versions;

/// What one subscription is sent of the feed.
#[derive(Clone, Debug)]
pub(super) struct Sends {
    of: Of,
    /// Whether its start asked to be sent what GitHub says as GitHub's observations, in its level's list and notices,
    /// rather than as `HubFrame::Github`, as its level sends it. Only a `Subscribe` start asks for them, and every
    /// level that sends anything of GitHub's sends each kind of it as an observation.
    github_observed: bool,
    /// The version of the client protocol its connection chose.
    version: u32,
}

/// Whose notices a subscription is sent.
#[derive(Clone, Debug)]
enum Of {
    /// Those of one level, as `Subscribe` follows it.
    Level(Level),
    /// Every notice but those of a workspace's level, which only `Subscribe` follows, as `FeedSubscribe` is sent them,
    /// so no client of the version it's kept for is sent the largest, such as a workspace's changed files, for every
    /// workspace.
    Feed,
}

impl Sends {
    /// What a subscription `Subscribe` started at `level` is sent, on a connection whose hello chose `version`, reading
    /// GitHub's observations if `github_observed` or its version reads every observation.
    pub(super) fn at(level: Level, github_observed: bool, version: u32) -> Self {
        Self { of: Of::Level(level), github_observed, version }
    }

    /// What a subscription `FeedSubscribe` started is sent, and what `RecordsList` lists, which only a client of
    /// version 3 sends, so in the forms from before observations.
    pub(super) fn feed() -> Self {
        Self { of: Of::Feed, github_observed: false, version: versions::CLIENTS.oldest }
    }

    /// Whether it's sent `fed`: a notice of its level, in a form its client reads, and that moves something its client
    /// reads, which it hasn't heard in a copy as new.
    pub(super) fn notice(&self, fed: &Fed) -> bool {
        let notice = &*fed.notice;
        let of = match &self.of {
            Of::Level(level) => notice.level().as_ref() == Some(level),
            Of::Feed => !matches!(notice.level(), Some(Level::Workspace { .. })),
        };
        let moves = match fed.moves {
            Moves::Anything => !fed.told_in_old_form || self.reads_every_observation(),
            Moves::OldFormOnly => !self.reads_every_observation(),
        };
        of && moves && self.sends(notice)
    }

    /// `listed`, a list of where a level stands, as it's sent: with only the observations its client reads as such.
    pub(super) fn listed(&self, listed: Listed) -> Listed {
        match listed {
            Listed::Hub(records) => Listed::Hub(Box::new(self.records(*records))),
            Listed::Workspace(WorkspaceObservations { workspace_id, observations, rev }) => {
                Listed::Workspace(WorkspaceObservations { workspace_id, observations: self.kept(observations), rev })
            }
            Listed::Reference(ReferenceObservations { reference, observations, rev }) => {
                Listed::Reference(ReferenceObservations { reference, observations: self.kept(observations), rev })
            }
            Listed::Session(records) => {
                Listed::Session(SessionRecords { observations: self.kept(records.observations), ..records })
            }
            listed @ Listed::Unknown => listed,
        }
    }

    /// Every record, as `RecordsList` and the hub's level list them, as it's sent: with only the observations its client
    /// reads as such.
    pub(super) fn records(&self, records: Records) -> Records {
        Records { observations: self.kept(records.observations), ..records }
    }

    /// Those of `observations` its client reads as such.
    fn kept(&self, observations: Vec<Observation>) -> Vec<Observation> {
        observations.into_iter().filter(|observation| self.keeps(&observation.key())).collect()
    }

    /// Whether it's sent what GitHub says as GitHub's observations, rather than as `HubFrame::Github`: where its start
    /// asked for them, and on a connection of a version that reads every observation.
    pub(super) fn github_observed(&self) -> bool {
        self.github_observed || self.reads_every_observation()
    }

    /// Whether its client reads every copy of what the hub only observes as an observation, and none in the form it
    /// replaced, so it's sent no copy of a record that moves only such a form.
    fn reads_every_observation(&self) -> bool {
        self.version >= EVERY_OBSERVATION_FROM
    }

    /// Whether it's sent `notice` in a form it reads: every notice but those of an observation its client reads in a
    /// form from before it, as [`Sends::notice`] asks.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "picks out the notices of an observation, and every other notice is sent whatever the form"
    )]
    fn sends(&self, notice: &HubNotice) -> bool {
        let key = match notice {
            HubNotice::Observation(observation) => observation.key(),
            HubNotice::ObservationRemoved { observation, .. } => observation.clone(),
            _ => return true,
        };
        self.keeps(&key)
    }

    /// Whether it's sent the observation `key`: where its client reads that kind as an observation, rather than in a
    /// form from before it, which it's sent instead, as [`ObservationKey::read_from`] says. One of a kind this build
    /// doesn't know is sent, though the hub makes none.
    fn keeps(&self, key: &ObservationKey) -> bool {
        match key.read_from() {
            Some(ReadFrom::Every) | None => true,
            Some(ReadFrom::GithubObserved) => self.github_observed(),
            Some(ReadFrom::EveryObservation) => self.reads_every_observation(),
        }
    }
}
