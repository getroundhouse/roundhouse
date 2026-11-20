use crate::named::named;
use crate::space::Space;

named! {
    /// What a bar holds, which says how far in from its ends that starts and ends. [`crate::bar`] makes one of a kind.
    BarKind {
        /// What's said of what's under it and what's done about it, such as the workspace's header, the pull request's
        /// bar or a file's path, starting where a tab's name does, so the bars over and under a row of tabs line up
        /// with its names.
        Words,
        /// A row of tabs, a little in from its start, so each tab's name starts where a bar's words do, with its edge
        /// under the tabs, so the line under the one showing covers it.
        Tabs,
    }
}

impl BarKind {
    /// How far in from its start what it holds starts, which a bar that draws itself takes too, such as the one under
    /// an image's comparison.
    pub fn start(self) -> Space {
        match self {
            BarKind::Words => Space::Large,
            BarKind::Tabs => Space::XSmall,
        }
    }

    /// How far in from its end what it holds ends: a button's room in from it, whatever it holds.
    pub const END: Space = Space::Medium;

    /// How far apart what it holds is.
    pub(super) fn gap(self) -> Space {
        match self {
            BarKind::Words => Space::Small,
            BarKind::Tabs => Space::XSmall,
        }
    }

    /// How the spec's table writes it: its name, and how far in it starts and ends.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 3] {
        let px = |space: Space| format!("{}px", f32::from(space.size()));
        [format!("{self:?}"), px(self.start()), px(Self::END)]
    }
}
