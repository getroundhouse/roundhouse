//! What each subscription is sent of the feed's notices, decided in one place, [`Sends::notice`], for `FeedSubscribe`
//! and for every level `Subscribe` follows: the notices of its level, in the form of what GitHub says it reads. Which
//! form that is, `HubFrame::Github`, as clients from before observations read it, or GitHub's observations, as
//! `docs/principles/observations.md` has them, one field decides, [`Sends::github_observed`], whatever request started
//! the subscription, so what decides which clients still read the old form changes in one place.

use protocol::client::HubNotice;
use protocol::client::listed::{Listed, ReferenceObservations, WorkspaceObservations};
use protocol::model::{Level, Observation, ObservationKey, Records};

/// What one subscription is sent of the feed.
#[derive(Clone, Debug)]
pub(super) struct Sends {
    of: Of,
    /// Whether it's sent what GitHub says as GitHub's observations, in its level's list and notices, rather than as
    /// `HubFrame::Github`, as its level sends it. Only a `Subscribe` start asks for them, and every level that sends
    /// anything of GitHub's sends each kind of it as an observation.
    github_observed: bool,
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
    /// What a subscription `Subscribe` started at `level` is sent, reading GitHub's observations if `github_observed`.
    pub(super) fn at(level: Level, github_observed: bool) -> Self {
        Self { of: Of::Level(level), github_observed }
    }

    /// What a subscription `FeedSubscribe` started is sent, and what `RecordsList` lists, which only a client of
    /// version 3 sends, so in the form from before observations.
    pub(super) fn feed() -> Self {
        Self { of: Of::Feed, github_observed: false }
    }

    /// Whether it's sent `notice`: one of its level, and of a form of what GitHub says it reads.
    pub(super) fn notice(&self, notice: &HubNotice) -> bool {
        let of = match &self.of {
            Of::Level(level) => notice.level().as_ref() == Some(level),
            Of::Feed => !matches!(notice.level(), Some(Level::Workspace { .. })),
        };
        of && self.sends(notice)
    }

    /// `listed`, a list of where a level stands, as it's sent: with only the observations of a form it reads.
    pub(super) fn listed(&self, listed: Listed) -> Listed {
        match listed {
            Listed::Hub(records) => Listed::Hub(Box::new(self.records(*records))),
            Listed::Workspace(WorkspaceObservations { workspace_id, observations, rev }) => {
                Listed::Workspace(WorkspaceObservations { workspace_id, observations: self.kept(observations), rev })
            }
            Listed::Reference(ReferenceObservations { reference, observations, rev }) => {
                Listed::Reference(ReferenceObservations { reference, observations: self.kept(observations), rev })
            }
            listed @ (Listed::Session(_) | Listed::Unknown) => listed,
        }
    }

    /// Every record, as `RecordsList` and the hub's level list them, as it's sent: with only the observations of a
    /// form it reads.
    pub(super) fn records(&self, records: Records) -> Records {
        Records { observations: self.kept(records.observations), ..records }
    }

    /// Those of `observations` of a form it reads.
    fn kept(&self, observations: Vec<Observation>) -> Vec<Observation> {
        observations.into_iter().filter(|observation| self.keeps(&observation.key())).collect()
    }

    /// Whether it's sent what GitHub says as GitHub's observations, rather than as `HubFrame::Github`.
    pub(super) fn github_observed(&self) -> bool {
        self.github_observed
    }

    /// Whether it's sent `notice` in the form of what GitHub says it reads: every notice but those of GitHub's
    /// observations, unless it reads them, as [`Sends::notice`] asks.
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

    /// Whether it's sent the observation `key` in the form of what GitHub says it reads.
    fn keeps(&self, key: &ObservationKey) -> bool {
        self.github_observed || !githubs(key)
    }
}

/// Whether `key` is of what GitHub says, which a client from before observations is sent as `HubFrame::Github`.
fn githubs(key: &ObservationKey) -> bool {
    match key {
        ObservationKey::WorkspacePullRequest { .. }
        | ObservationKey::WorkspacePullRequestDetail { .. }
        | ObservationKey::WorkspaceRepositoryUnseen { .. }
        | ObservationKey::Reference { .. } => true,
        ObservationKey::WorkspaceLines { .. } | ObservationKey::WorkspaceChanges { .. } | ObservationKey::Unknown => {
            false
        }
    }
}
