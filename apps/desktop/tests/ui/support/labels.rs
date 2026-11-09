//! Finding what's shown by its accessible label, which is what assistive apps read, rather than by element ID.

use gpui_kit::test::TestQueryExt as _;
use gpui_kit::{AppContext as _, ElementId, Window};

use super::Ui;

/// The ID of the one element labelled `label`, to act on it.
pub(crate) fn labelled(window: &Window, label: &str) -> ElementId {
    window.find_by_label(label).path().last().unwrap().clone()
}

impl Ui<'_> {
    /// Clicks the one element labelled `label`, such as a menu item.
    pub(crate) fn click_label(&mut self, label: &str) {
        self.frame();
        let id = self.cx.update_window(self.handle, |_, window, _| labelled(window, label)).unwrap();
        self.click(id);
    }

    /// Whether the last frame labelled the element with ID `id`, or something inside it, `label`.
    pub(crate) fn shows_label(&mut self, id: &str, label: &str) -> bool {
        let element = ElementId::Name(id.to_string().into());
        self.frame();
        self.cx
            .update_window(self.handle, |_, window, _| {
                window.find_all_by_label(label).iter().any(|found| found.path().contains(&element))
            })
            .unwrap()
    }
}
