use gpui_kit::component::switch::Switch;
use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};

use crate::hand::Off as _;

/// What a toggle does as it's flipped: it's given whether it's on now.
type Flip = Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>;

/// A toggle, found by `id`, that's on while `on`. Flipping it says so to what [`Toggle::on_flip`] is given, and the
/// place that holds the setting draws it again from what it holds.
pub fn toggle(id: impl Into<ElementId>, on: bool) -> Toggle {
    Toggle { id: id.into(), on, off: false, label: None, flip: None }
}

/// A setting that's on or off, which [`toggle`] makes.
#[derive(IntoElement)]
pub struct Toggle {
    id: ElementId,
    on: bool,
    off: bool,
    label: Option<SharedString>,
    flip: Option<Flip>,
}

impl Toggle {
    /// What an assistive app calls it, such as the setting's name.
    #[must_use]
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Greyed out while `off`, so it can't be flipped, such as while what it's for can't be read or changed. Whether
    /// it's on still shows.
    #[must_use]
    pub fn off(mut self, off: bool) -> Self {
        self.off = off;
        self
    }

    /// What flipping it does, given whether it's on now.
    #[must_use]
    pub fn on_flip(mut self, flip: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.flip = Some(Box::new(flip));
        self
    }

    /// The kit's switch, drawn as this toggle: with the hand while it can be flipped, and the arrow while it's off.
    pub(super) fn drawn(self) -> Switch {
        let switch = Switch::new(self.id).checked(self.on).off_while(self.off);
        let switch = match self.label {
            Some(label) => switch.accessibility_label(label),
            None => switch,
        };
        match self.flip {
            Some(flip) => switch.on_click(move |on: &bool, window, cx| flip(on, window, cx)),
            None => switch,
        }
    }
}

impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.drawn()
    }
}
