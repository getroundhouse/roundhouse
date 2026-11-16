//! Only the selected hub may present a dialog or take the window's focus. Background replies still update their
//! own records and drafts; requests to present them wait here until that hub is selected again.

use gpui_kit::{Context, Window};

use super::AppView;

type Present = Box<dyn FnOnce(&mut AppView, &mut Window, &mut Context<AppView>)>;
pub(super) type Pending = Vec<Present>;

impl AppView {
    pub(super) fn present(
        &mut self,
        cx: &mut Context<Self>,
        present: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.presentation.push(Box::new(present));
        let (handle, view) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = view.update(cx, |view, cx| view.present_pending(window, cx));
            });
        });
    }

    pub(super) fn present_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        for present in std::mem::take(&mut self.presentation) {
            present(self, window, cx);
        }
    }
}
