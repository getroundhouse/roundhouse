//! How the file tab shows a diff: in one column, side by side, or the whole file. Every workspace shares the choice
//! until the app quits, as it shares the sidebar's, and the switch for it is at the right end of the file tab's bar.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex};
use gpui_kit::{AnyElement, App, Global, IntoElement as _, ParentElement as _, Styled as _, px};
use look::Hand as _;

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

impl DiffView {
    /// In the order the switch shows them.
    pub const ALL: [Self; 3] = [Self::Unified, Self::Split, Self::WholeFile];

    /// The ID of its button on the switch.
    pub fn id(self) -> &'static str {
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

    /// Whether the old side is beside the new one.
    pub(super) fn split(self) -> bool {
        self == Self::Split
    }

    /// Whether every line shows, with nothing folded.
    pub(super) fn everything(self) -> bool {
        self == Self::WholeFile
    }
}

/// The view chosen, which every workspace shares until the app quits.
pub(super) struct Chosen(DiffView);

impl Global for Chosen {}

/// The view chosen last, or the first, before any has been.
pub(super) fn chosen(cx: &App) -> DiffView {
    cx.try_global::<Chosen>().map(|chosen| chosen.0).unwrap_or_default()
}

/// The three views, as one switch, with the one showing pressed in.
pub(super) fn switch(cx: &App) -> AnyElement {
    let showing = chosen(cx);
    h_flex()
        .flex_none()
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(6.))
        .bg(look::colors(cx).raised)
        .children(DiffView::ALL.map(|view| {
            Button::new(view.id())
                .hand()
                .ghost()
                .xsmall()
                .icon(view.icon())
                .selected(view == showing)
                .tooltip(view.name())
                .on_click(move |_, _, cx| {
                    if chosen(cx) != view {
                        cx.set_global(Chosen(view));
                    }
                })
        }))
        .into_any_element()
}
