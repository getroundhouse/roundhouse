//! What's selected of a text drawn elsewhere, such as a command, which is highlighted as shell. The kit's
//! `SelectableText` draws its text itself, plain, so this takes the text's layout instead, and goes under the text,
//! filling what holds it.

use gpui_kit::base::{TextSelectionRun, Theme};
use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, TextLayout, Window, fill};

use crate::window_selection::{Selectable, highlight};

/// What's selected of the text laid out in `layout`.
#[derive(IntoElement)]
pub(crate) struct SelectableLayout {
    /// Keeps what's selected from one frame to the next, so it's this text's alone.
    id: ElementId,
    /// What's laid out.
    text: SharedString,
    layout: TextLayout,
}

impl SelectableLayout {
    pub(crate) fn new(id: impl Into<ElementId>, text: SharedString, layout: TextLayout) -> Self {
        Self { id: id.into(), text, layout }
    }
}

impl RenderOnce for SelectableLayout {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (text, layout) = (self.text, self.layout);
        Selectable::kept(&self.id, window, cx)
            .under(move |selection, window, cx| paint(selection, &text, &layout, window, cx))
    }
}

/// Paints what's selected of the text, and keeps it for a copy.
fn paint(selection: &Selectable, text: &SharedString, layout: &TextLayout, window: &mut Window, cx: &mut App) {
    let projection = selection.project(&[TextSelectionRun::new(text.clone(), layout.clone(), layout.bounds())], cx);
    let range = projection.ranges().first().cloned().flatten();
    if let Some(range) = &range
        && let (Some(start), Some(end)) = (layout.position_for_index(range.start), layout.position_for_index(range.end))
    {
        let color = Theme::global(cx).tokens.colors.selection;
        for bounds in highlight(start, end, layout.bounds(), layout.line_height()) {
            window.paint_quad(fill(bounds, color));
        }
    }
    selection.keep(range.map(|range| text[range].to_string()).unwrap_or_default());
}
