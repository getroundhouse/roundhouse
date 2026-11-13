//! Where a workspace's work stands, in GitHub's colours, which the people Roundhouse is for already know. The mark's
//! fan and the pull request's bar draw from the same colours, so each means the same in both.

use gpui_kit::Hsla;

use crate::colors::Colors;

/// How strongly a pull request's colour shows through the bar behind it.
const TINT: f32 = 0.12;
/// How strongly it shows in the outline of a button that opens the pull request.
const EDGE: f32 = 0.35;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Work {
    NoPr,
    Draft,
    /// Open, with something to do before it can merge.
    Open,
    /// Open, with nothing in the way of merging it.
    Ready,
    /// In the merge queue, which merges it when its turn comes.
    Queued,
    Merged,
    Closed,
}

impl Work {
    /// The fan's colour, whatever else the mark says. An open pull request is green there whether or not something's
    /// in the way of merging it, as GitHub shows it, since a mark is too small to say what. One in the merge queue is
    /// yellow, as the bar is.
    pub(crate) fn fan(self, colors: &Colors) -> Hsla {
        match self {
            Work::NoPr => colors.text_faint,
            Work::Draft => colors.pr_draft,
            Work::Open | Work::Ready => colors.pr_open,
            Work::Queued => colors.pr_queued,
            Work::Merged => colors.pr_merged,
            Work::Closed => colors.pr_closed,
        }
    }

    /// Its own colour in the bar, and the main action's fill, with the theme's dark text on it as every main action
    /// has. `None` for one that's either not a pull request yet or has something to do. Those two take the theme's,
    /// since neither is good or bad news.
    pub fn hue(self, colors: &Colors) -> Option<Hsla> {
        match self {
            Work::NoPr | Work::Open => None,
            Work::Draft => Some(colors.pr_draft),
            Work::Ready => Some(colors.pr_open),
            Work::Queued => Some(colors.pr_queued),
            Work::Merged => Some(colors.pr_merged),
            Work::Closed => Some(colors.pr_closed),
        }
    }

    /// What the bar says.
    pub fn text(self, colors: &Colors) -> Hsla {
        self.hue(colors).unwrap_or(colors.text)
    }

    /// The bar behind it. Something to do shows on a raised surface, so the bar still stands out from the sidebar.
    pub fn tint(self, colors: &Colors) -> Hsla {
        match (self, self.hue(colors)) {
            (_, Some(hue)) => hue.opacity(TINT),
            (Work::Open, None) => colors.raised,
            (_, None) => colors.sidebar,
        }
    }

    /// The outline of the button that opens the pull request.
    pub fn edge(self, colors: &Colors) -> Hsla {
        self.hue(colors).map_or(colors.control_edge, |hue| hue.opacity(EDGE))
    }
}

#[cfg(test)]
mod tests {
    use super::Work;
    use crate::colors::Colors;

    #[test]
    fn the_fan_is_the_bars_colour_wherever_the_bar_has_one() {
        let colors = Colors::dark();

        for work in [Work::Draft, Work::Ready, Work::Queued, Work::Merged, Work::Closed] {
            assert_eq!(Some(work.fan(&colors)), work.hue(&colors), "{work:?}");
        }
    }

    #[test]
    fn the_fan_tells_the_merge_queue_from_any_other_open_pull_request() {
        let colors = Colors::dark();
        let fan = |work: Work| work.fan(&colors);

        assert_eq!(fan(Work::Open), fan(Work::Ready), "open is green whether or not something's in the way");
        assert_ne!(fan(Work::Queued), fan(Work::Ready));
        assert_ne!(fan(Work::Open), fan(Work::NoPr));
    }
}
