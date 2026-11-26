use gpui_kit::base::TextViewStyle;
use gpui_kit::base::text::editor_host::RenderContext;
use gpui_kit::{HighlightStyle, StyleRefinement, Styled as _};

use crate::find::Part;

/// What's marked strong in Markdown is `look::STRONG`, semibold, as its headings are, where the kit's own is bold. A
/// thought keeps the surrounding text's weight even where its Markdown asks for bold, so its progress notes stay
/// quieter than the answer. Both have inline code and a list's items as `look` has them. This changes only how the kit draws it, never its source or parsed blocks.
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
        let style = self.styled(context.style().clone());
        context.with_style(style)
    }

    /// `style`, with what's strong, its headings, its inline code and its lists as this kind of text has them.
    fn styled(self, style: TextViewStyle) -> TextViewStyle {
        let style = style.with_inline_code_scale(look::INLINE_CODE).with_list_item_gap(look::LIST_ITEM_GAP);
        match self {
            Self::Markdown => {
                let strong = HighlightStyle { font_weight: Some(look::STRONG), ..HighlightStyle::default() };
                style.with_strong(strong).with_heading(|_| StyleRefinement::default().font_weight(look::STRONG))
            }
            Self::Thinking => style.with_strong(HighlightStyle::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::FontWeight;
    use gpui_kit::base::TextViewStyle;

    use super::Style;

    /// The agent's bold and its headings stand a step past the app's own emphasis, as `docs/spec/look.md` says, where
    /// the kit's bold would be heavier still.
    #[test]
    fn what_the_agent_marks_strong_and_its_headings_are_semibold() {
        let style = Style::Markdown.styled(TextViewStyle::default());

        assert_eq!(style.strong().font_weight, Some(FontWeight::SEMIBOLD));
        for level in 1..=6 {
            assert_eq!(style.heading(level).text.font_weight, Some(FontWeight::SEMIBOLD), "a heading of level {level}");
        }
    }

    #[test]
    fn inline_code_and_a_lists_items_are_as_look_has_them() {
        for kind in [Style::Markdown, Style::Thinking] {
            let style = kind.styled(TextViewStyle::default().with_inline_code_scale(1.).with_list_item_gap(1.));

            let drawn = (style.inline_code_scale(), style.list_item_gap());
            assert_eq!(drawn, (look::INLINE_CODE, look::LIST_ITEM_GAP));
        }
    }

    #[test]
    fn a_thought_keeps_the_weight_around_it_where_it_asks_for_bold() {
        let style = Style::Thinking.styled(TextViewStyle::default());

        assert_eq!(style.strong().font_weight, None);
    }
}
