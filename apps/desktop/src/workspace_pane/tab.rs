//! The close button every tab in the workspace's row has, whatever it shows, so each closes the same way. The tab is
//! `look::tab`, at the even width, so the row reads as one row of tabs.

use gpui_kit::component::IconName;
use gpui_kit::{Context, ElementId, Window};

use super::WorkspacePane;

/// A tab's close button, which closes it without the tab under it taking the click too, and showing what's closing.
pub(super) fn close_button(
    id: impl Into<ElementId>,
    tooltip: &'static str,
    cx: &mut Context<WorkspacePane>,
    close: impl Fn(&mut WorkspacePane, &mut Window, &mut Context<WorkspacePane>) + 'static,
) -> look::Button {
    look::icon_button(id, look::ButtonKind::Borderless, look::ControlSize::Small, IconName::Close)
        .tooltip(tooltip)
        .on_click(cx.listener(move |t, _, window, cx| {
            cx.stop_propagation();
            close(t, window, cx);
        }))
}
