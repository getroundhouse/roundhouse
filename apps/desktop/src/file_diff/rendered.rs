//! A changed Markdown file's diff, rendered, as the file tab shows the file: what the change removed struck through on
//! red, and what it added on green, drawn by the transcript's `Document` from the file whole, as it was and as it is,
//! once the sides have it.

use std::sync::Arc;

#[cfg(feature = "testing")]
use gpui_kit::App;
use gpui_kit::{AnyElement, Context, IntoElement as _};

use super::FileDiff;
#[cfg(feature = "testing")]
use super::view::DiffView;

impl FileDiff {
    /// What a copy of the whole of a Markdown file's diff has, while it shows it rendered, or `None` while it doesn't.
    #[cfg(feature = "testing")]
    pub fn rendered(&self, cx: &App) -> Option<String> {
        (self.view(cx) == DiffView::Rendered && self.rendered.is_some()).then(|| self.document.read(cx).copied_whole())
    }

    /// The file rendered, with what it removed and added marked, once it's known whole, as it was and as it is.
    pub(super) fn rendered_file(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let versions = self.shown.as_ref().and_then(|shown| shown.sides.versions.clone());
        let Some(versions) = versions else {
            return if self.is_ready() {
                look::nothing_to_show("This file can't be read whole, so it can't show rendered").into_any_element()
            } else {
                look::nothing_to_show("Loading…").into_any_element()
            };
        };
        if self.rendered.as_ref().is_none_or(|rendered| !Arc::ptr_eq(rendered, &versions)) {
            let path = self.path.clone().unwrap_or_default();
            let (was, is) = (versions.was.clone(), versions.is.clone());
            self.document.update(cx, |document, cx| document.show_changes(path, was, is, cx));
            self.rendered = Some(versions);
        }
        self.document.clone().into_any_element()
    }
}
