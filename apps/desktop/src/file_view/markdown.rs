//! How the file tab shows a Markdown file: rendered, as the chat draws Markdown, or as its source, its lines numbered
//! and highlighted as any file's are. Every workspace shares the choice until the app quits, as it shares the diff's,
//! and the switch for it is at the right end of the file tab's bar.

use gpui_kit::assets::IconName as Lucide;

use crate::shared_choice::OnASwitch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MarkdownView {
    /// Rendered, in the chat's column.
    #[default]
    Preview,
    /// Its lines, as any other file's.
    Source,
}

impl OnASwitch for MarkdownView {
    const ALL: &'static [Self] = &[Self::Preview, Self::Source];

    fn id(self) -> &'static str {
        match self {
            Self::Preview => "markdown-preview",
            Self::Source => "markdown-source",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::Source => "Source",
        }
    }

    fn icon(self) -> Lucide {
        match self {
            Self::Preview => Lucide::Eye,
            Self::Source => Lucide::CodeXml,
        }
    }
}

/// Whether the file at `path` is Markdown, by its name, as its language is found for highlighting.
pub(crate) fn is_markdown(path: &str) -> bool {
    let language = look::language(path);
    language.eq_ignore_ascii_case("md") || language.eq_ignore_ascii_case("markdown")
}
