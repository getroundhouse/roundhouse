//! Where a workspace's work stands, in GitHub's colours, which the people Roundhouse is for already know. The mark's
//! fan and the pull request's bar draw from the same colours, so each means the same in both.

use gpui_kit::Hsla;
use gpui_kit::assets::IconName;

use crate::colors::Colors;

/// How strongly a pull request's colour shows through the bar behind it.
const TINT: f32 = 0.12;
/// How strongly it shows in the outline of a button that opens the pull request.
const EDGE: f32 = 0.35;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Work {
    NoPr,
    Draft,
    /// Open, whatever's in the way of merging it, which a [`Sign`] says.
    Open,
    /// In the merge queue, which merges it when its turn comes.
    Queued,
    Merged,
    Closed,
}

impl Work {
    /// The fan's colour, whatever else the mark says, which is the bar's, and the faint text's where there's no pull
    /// request yet.
    pub(crate) fn fan(self, colors: &Colors) -> Hsla {
        self.hue(colors).unwrap_or(colors.text_faint)
    }

    /// Its own colour in the bar, and the main action's fill, with the theme's dark text on it as every main action
    /// has. `None` for a workspace with no pull request yet, which takes the theme's, since that's neither good nor bad
    /// news.
    pub fn hue(self, colors: &Colors) -> Option<Hsla> {
        match self {
            Work::NoPr => None,
            Work::Draft => Some(colors.pr_draft),
            Work::Open => Some(colors.pr_open),
            Work::Queued => Some(colors.pr_queued),
            Work::Merged => Some(colors.pr_merged),
            Work::Closed => Some(colors.pr_closed),
        }
    }

    /// What the bar says.
    pub fn text(self, colors: &Colors) -> Hsla {
        self.hue(colors).unwrap_or(colors.text)
    }

    /// The bar behind it.
    pub fn tint(self, colors: &Colors) -> Hsla {
        self.hue(colors).map_or(colors.sidebar, |hue| hue.opacity(TINT))
    }

    /// The outline of the button that opens the pull request.
    pub fn edge(self, colors: &Colors) -> Hsla {
        self.hue(colors).map_or(colors.control_edge, |hue| hue.opacity(EDGE))
    }
}

/// What kind of thing is in the way of merging a pull request, which the bar says with an icon before its words, and
/// the header beside its number: something to fix, something it waits on, or nothing at all. Work in the worktree has
/// none, since its button says what to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sign {
    /// It can't merge until someone fixes it, such as a conflict, failed checks or changes requested. Its words are in
    /// the error colour too, so it stands out on a bar of any colour.
    Problem,
    /// It waits on GitHub or someone else, such as checks running or a review.
    Waiting,
    /// Nothing is in the way.
    Ready,
}

impl Sign {
    pub fn icon(self) -> IconName {
        match self {
            Sign::Problem => IconName::CircleX,
            Sign::Waiting => IconName::LoaderCircle,
            Sign::Ready => IconName::CircleCheck,
        }
    }

    /// The icon's colour, on a bar of `work`'s.
    pub fn color(self, work: Work, colors: &Colors) -> Hsla {
        match self {
            Sign::Problem => colors.error,
            Sign::Waiting => colors.text_muted,
            Sign::Ready => work.text(colors),
        }
    }

    /// The colour of the words beside it, on a bar of `work`'s: a problem's in its own, and the rest in the bar's.
    pub fn words(sign: Option<Self>, work: Work, colors: &Colors) -> Hsla {
        match sign {
            Some(Sign::Problem) => colors.error,
            Some(Sign::Waiting | Sign::Ready) | None => work.text(colors),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Work;
    use crate::colors::Colors;

    #[test]
    fn the_fan_is_the_bars_colour_wherever_the_bar_has_one() {
        let colors = Colors::dark();

        for work in [Work::Draft, Work::Open, Work::Queued, Work::Merged, Work::Closed] {
            assert_eq!(Some(work.fan(&colors)), work.hue(&colors), "{work:?}");
        }
    }

    #[test]
    fn the_fan_tells_the_merge_queue_from_any_other_open_pull_request() {
        let colors = Colors::dark();
        let fan = |work: Work| work.fan(&colors);

        assert_ne!(fan(Work::Queued), fan(Work::Open));
        assert_ne!(fan(Work::Open), fan(Work::NoPr));
    }
}
