//! Finding what the window drew by the label it gives assistive apps, rather than by element ID, or by the start of
//! its ID, for what a test can't know the ID of, such as a tool call's line.

use gpui_kit::test::{ElementSnapshot, TestQueryExt as _};
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

    /// Each element drawn with this label, in the order it was drawn.
    pub(crate) fn labelled(&mut self, label: &str) -> Vec<ElementSnapshot> {
        self.frame();
        self.cx.update_window(self.handle, |_, window, _| window.find_all_by_label(label)).unwrap()
    }

    /// Clicks the element at `position` among those with this label, as [`Ui::labelled`] finds them.
    pub(crate) fn click_labelled(&mut self, label: &str, position: usize) {
        let found = self.labelled(label);
        let element = found.get(position).unwrap_or_else(|| panic!("no element {position} labelled {label}"));
        self.click(element.path().last().unwrap().clone());
    }

    /// Each element drawn whose ID starts with `part`. The transcript gives each item's parts an ID of the part's
    /// name and then the item's identity, such as `output/` and then the call's.
    pub(crate) fn shown(&mut self, part: &str) -> Vec<ElementId> {
        self.frame();
        self.cx
            .update_window(self.handle, |_, window, _| {
                let ids = window.elements().into_iter().filter_map(|e| e.path().last().cloned());
                ids.filter(|id| matches!(id, ElementId::Name(name) if name.starts_with(part))).collect()
            })
            .unwrap()
    }
}
