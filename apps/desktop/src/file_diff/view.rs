//! How the file tab shows a diff: in one column, side by side, or the whole file. Every workspace shares the choice
//! until the app quits, as it shares the sidebar's, and the switch for it is at the right end of the file tab's bar.

use gpui_kit::assets::IconName as Lucide;

use crate::shared_choice::OnASwitch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffView {
    /// One column, with the file before and after a change one above the other, and the lines git left out folded.
    #[default]
    Unified,
    /// The file before on the left and after on the right, folded as in one column.
    Split,
    /// Every line of the file, in one column, with its changes where they are.
    WholeFile,
}

impl OnASwitch for DiffView {
    const ALL: &'static [Self] = &[Self::Unified, Self::Split, Self::WholeFile];

    fn id(self) -> &'static str {
        match self {
            Self::Unified => "diff-unified",
            Self::Split => "diff-split",
            Self::WholeFile => "diff-whole-file",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Unified => "Unified",
            Self::Split => "Split",
            Self::WholeFile => "Whole file",
        }
    }

    fn icon(self) -> Lucide {
        match self {
            Self::Unified => Lucide::TextAlignStart,
            Self::Split => Lucide::Columns2,
            Self::WholeFile => Lucide::FileText,
        }
    }
}

impl DiffView {
    /// Whether the old side is beside the new one.
    pub(super) fn split(self) -> bool {
        self == Self::Split
    }

    /// Whether every line shows, with nothing folded.
    pub(super) fn everything(self) -> bool {
        self == Self::WholeFile
    }
}
