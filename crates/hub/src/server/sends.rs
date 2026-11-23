//! What each subscription is sent of the feed's notices, decided in one place, [`Sends::notice`], for `FeedSubscribe`
//! and for every level `Subscribe` follows: the notices of its level, in the form of what GitHub says it reads. Which
//! form that is, `HubFrame::Github`, as clients from before observations read it, or GitHub's observations, as
//! `docs/principles/observations.md` has them, one predicate decides, [`GithubForms::of`], whatever request started the
//! subscription, so what decides which clients still read the old form changes in one place.

use protocol::client::HubNotice;
use protocol::model::{Level, ObservationKey};

/// What one subscription is sent of the feed.
#[derive(Clone, Debug)]
pub(super) struct Sends {
    of: Of,
    forms: GithubForms,
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
    /// What a subscription `Subscribe` started at `level`, as `started`, is sent.
    pub(super) fn at(level: Level, started: Started) -> Self {
        let forms = GithubForms::of(started, &level);
        Self { of: Of::Level(level), forms }
    }

    /// What a subscription `FeedSubscribe` started is sent.
    pub(super) fn feed() -> Self {
        Self { of: Of::Feed, forms: GithubForms::of(Started::Before, &Level::Hub) }
    }

    /// Whether it's sent `notice`: one of its level, and of a form of what GitHub says it reads.
    pub(super) fn notice(&self, notice: &HubNotice) -> bool {
        let of = match &self.of {
            Of::Level(level) => notice.level().as_ref() == Some(level),
            Of::Feed => !matches!(notice.level(), Some(Level::Workspace { .. })),
        };
        of && self.forms.sends(notice)
    }

    /// The forms of what GitHub says it's sent, for what sends it more than the feed's notices.
    pub(super) fn forms(&self) -> GithubForms {
        self.forms
    }
}

/// The forms of what GitHub says that one subscription is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GithubForms {
    /// `HubFrame::Github`, as its level sends it, at a level that sends it.
    pub(super) updates: bool,
    /// GitHub's observations, in the level's list and notices.
    pub(super) observations: bool,
}

/// How a subscription was started, which is all [`GithubForms::of`] goes by.
#[derive(Clone, Copy, Debug)]
pub(super) enum Started {
    /// By `Subscribe`, with what the start says of the form it reads.
    Subscribe { github_observed: bool },
    /// By a request from before `Subscribe`, such as `FeedSubscribe`, which only a client of version 3 sends.
    Before,
}

impl GithubForms {
    /// The forms of what GitHub says that a subscription of `level`, started as `started`, is sent: GitHub's
    /// observations where the start asks for them and the hub sends every kind of the level as one, and
    /// `HubFrame::Github` otherwise. So each subscription is sent one form, and a client from before observations is
    /// sent what it was.
    pub(super) fn of(started: Started, level: &Level) -> Self {
        let observed = match started {
            Started::Subscribe { github_observed } => github_observed && observed_at(level),
            Started::Before => false,
        };
        Self { updates: !observed, observations: observed }
    }

    /// Whether a subscription sent these forms is sent `notice`: every notice but those of GitHub's observations,
    /// unless it's sent them, as [`Sends::notice`] asks.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "picks out the notices of an observation, and every other notice is sent whatever the forms"
    )]
    fn sends(self, notice: &HubNotice) -> bool {
        let key = match notice {
            HubNotice::Observation(observation) => observation.key(),
            HubNotice::ObservationRemoved { observation, .. } => observation.clone(),
            _ => return true,
        };
        self.observations || !githubs(&key)
    }
}

/// Whether the hub sends every kind of what GitHub says at `level` as an observation, so a subscription there can be
/// sent them in place of `HubFrame::Github`.
fn observed_at(level: &Level) -> bool {
    match level {
        Level::Reference { .. } => true,
        // The hub's level sends each workspace's pull request as `HubFrame::Github` alone, with its checks.
        Level::Hub | Level::Workspace { .. } | Level::Session { .. } | Level::Terminal { .. } | Level::Unknown => false,
    }
}

/// Whether `key` is of what GitHub says, which a client from before observations is sent as `HubFrame::Github`.
fn githubs(key: &ObservationKey) -> bool {
    match key {
        ObservationKey::Reference { .. } => true,
        ObservationKey::WorkspaceLines { .. } | ObservationKey::WorkspaceChanges { .. } | ObservationKey::Unknown => {
            false
        }
    }
}
