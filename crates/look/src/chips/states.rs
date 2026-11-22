use gpui_kit::assets::IconName;
use gpui_kit::{App, Hsla};

use crate::colors::colors;
use crate::named::named;
use crate::work::Work;

named! {
    /// Where an issue stands on GitHub, which its chip's mark shows, as GitHub draws it, in the colour of the pull
    /// request's state it's most like.
    IssueState {
        /// Open: a dot in a circle, in an open pull request's green.
        Open,
        /// Closed as done: a tick in a circle, in a merged pull request's purple, as GitHub draws it.
        Done,
        /// Closed as not planned, or as a duplicate: a slashed circle, in a draft's grey.
        NotPlanned,
    }
}

named! {
    /// Where a pull request stands on GitHub, which its chip's mark shows, as GitHub draws it, in the colour the
    /// sidebar's fan gives it.
    PullRequestState {
        /// Open, ready or not: its branch's mark, in green.
        Open,
        /// A draft: a dotted branch, in grey.
        Draft,
        /// In the merge queue: its branch's mark, as it's still open, in the queue's yellow, as the bar is.
        Queued,
        /// Merged: the merge's mark, in purple.
        Merged,
        /// Closed without merging: a struck branch, in red.
        Closed,
    }
}

/// Where an issue or a pull request stands, which [`crate::Chip::state`] takes: an issue's state for an issue's
/// chip, and a pull request's for a pull request's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubState {
    Issue(IssueState),
    PullRequest(PullRequestState),
}

impl IssueState {
    /// The icon its chip's mark is, which a card's heading about it starts with too.
    pub const fn icon(self) -> IconName {
        match self {
            IssueState::Open => IconName::CircleDot,
            IssueState::Done => IconName::CircleCheck,
            IssueState::NotPlanned => IconName::CircleSlash,
        }
    }

    /// The pull request's state whose colour it takes.
    const fn colored_as(self) -> PullRequestState {
        match self {
            IssueState::Open => PullRequestState::Open,
            IssueState::Done => PullRequestState::Merged,
            IssueState::NotPlanned => PullRequestState::Draft,
        }
    }

    /// Its colour, which its chip's mark and the word for it on a card share.
    pub fn color(self, cx: &App) -> Hsla {
        self.colored_as().color(cx)
    }

    /// How the spec's table writes it: its name, its mark, and the pull request's state whose colour it takes.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 3] {
        [format!("{self:?}"), format!("{:?}", self.icon()), format!("{:?}", self.colored_as())]
    }
}

impl PullRequestState {
    /// The icon its chip's mark is, which a card's heading about it starts with too.
    pub const fn icon(self) -> IconName {
        match self {
            PullRequestState::Open | PullRequestState::Queued => IconName::GitPullRequest,
            PullRequestState::Draft => IconName::GitPullRequestDraft,
            PullRequestState::Merged => IconName::GitMerge,
            PullRequestState::Closed => IconName::GitPullRequestClosed,
        }
    }

    /// The pull request's work, as the sidebar's fan colours it.
    const fn work(self) -> Work {
        match self {
            PullRequestState::Open => Work::Open,
            PullRequestState::Draft => Work::Draft,
            PullRequestState::Queued => Work::Queued,
            PullRequestState::Merged => Work::Merged,
            PullRequestState::Closed => Work::Closed,
        }
    }

    /// Its colour, the sidebar's fan's for it, which its chip's mark and the word for it on a card share.
    pub fn color(self, cx: &App) -> Hsla {
        self.work().fan(colors(cx))
    }

    /// How the spec's table writes it: its name, its mark, and the work whose colour it takes.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 3] {
        [format!("{self:?}"), format!("{:?}", self.icon()), format!("{:?}", self.work())]
    }
}

impl GithubState {
    /// The icon its chip's mark is.
    pub const fn icon(self) -> IconName {
        match self {
            GithubState::Issue(state) => state.icon(),
            GithubState::PullRequest(state) => state.icon(),
        }
    }

    /// Its colour.
    pub fn color(self, cx: &App) -> Hsla {
        match self {
            GithubState::Issue(state) => state.color(cx),
            GithubState::PullRequest(state) => state.color(cx),
        }
    }
}

impl From<IssueState> for GithubState {
    fn from(state: IssueState) -> Self {
        GithubState::Issue(state)
    }
}

impl From<PullRequestState> for GithubState {
    fn from(state: PullRequestState) -> Self {
        GithubState::PullRequest(state)
    }
}
