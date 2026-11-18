//! How the file tab shows a diff: in one column, side by side, the whole file, or for a Markdown file, rendered. Every
//! workspace shares the choice until the app quits, as it shares the sidebar's, and the switch for it is at the right
//! end of the file tab's bar.

use gpui_kit::assets::IconName as Lucide;

use crate::file_view::is_markdown;
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
    /// A Markdown file rendered, as the file tab shows it, with each block it removed struck through on red, and each
    /// it added on green. Another file shows in one column while it's chosen.
    Rendered,
}

impl OnASwitch for DiffView {
    const ALL: &'static [Self] = &[Self::Unified, Self::Split, Self::WholeFile, Self::Rendered];

    fn id(self) -> &'static str {
        match self {
            Self::Unified => "diff-unified",
            Self::Split => "diff-split",
            Self::WholeFile => "diff-whole-file",
            Self::Rendered => "diff-rendered",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Unified => "Unified",
            Self::Split => "Split",
            Self::WholeFile => "Whole file",
            Self::Rendered => "Rendered",
        }
    }

    fn icon(self) -> Lucide {
        match self {
            Self::Unified => Lucide::TextAlignStart,
            Self::Split => Lucide::Columns2,
            Self::WholeFile => Lucide::FileText,
            Self::Rendered => Lucide::Eye,
        }
    }
}

impl DiffView {
    /// Those any file's lines can show as.
    const OF_LINES: &'static [Self] = &[Self::Unified, Self::Split, Self::WholeFile];

    /// How the file at `path` shows when this is chosen: rendered only if it's Markdown, and otherwise in one column.
    pub(super) fn of(self, path: &str) -> Self {
        if self == Self::Rendered && !is_markdown(path) { Self::Unified } else { self }
    }

    /// The choices for the file at `path`: Rendered too, if it's Markdown.
    pub(super) fn for_file(path: &str) -> &'static [Self] {
        if is_markdown(path) { Self::ALL } else { Self::OF_LINES }
    }

    /// Whether the old side is beside the new one.
    pub(super) fn split(self) -> bool {
        self == Self::Split
    }

    /// Whether every line shows, with nothing folded.
    pub(super) fn everything(self) -> bool {
        self == Self::WholeFile
    }
}
