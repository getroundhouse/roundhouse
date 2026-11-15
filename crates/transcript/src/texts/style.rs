use gpui_kit::HighlightStyle;
use gpui_kit::base::text::editor_host::RenderContext;

use crate::find::Part;

/// A thought keeps the surrounding text's weight even where its Markdown asks for bold, so its progress notes
/// stay quieter than the answer. This changes only how the kit draws it, never its source or parsed blocks.
#[derive(Clone, Copy)]
pub(crate) enum Style {
    Markdown,
    Thinking,
}

impl Style {
    pub(crate) fn for_part(part: Part) -> Self {
        if part == Part::Thinking { Self::Thinking } else { Self::Markdown }
    }

    pub(super) fn apply(self, context: RenderContext) -> RenderContext {
        match self {
            Self::Markdown => context,
            Self::Thinking => {
                let style = context.style().clone().with_strong(HighlightStyle::default());
                context.with_style(style)
            }
        }
    }
}
