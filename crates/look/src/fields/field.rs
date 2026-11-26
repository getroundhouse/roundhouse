use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Icon, IconName, ThemeStyled as _, h_flex};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, Focusable as _, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, RenderOnce, SharedString, Styled as _, TestSupportExt as _, Window, div,
    prelude::FluentBuilder as _,
};

use super::FieldKind;
use crate::{ControlSize, Corner, EDGE, IconSizing as _, Shaped as _, Space, Strength, Typeset as _, colors};

/// A field to type a line of text in, which `input` holds, of `kind` and `size`, found by `id`. It's as tall as its
/// size and leaves its size's room at each end, so it's as tall as the buttons beside it, and what's typed in it is
/// its size's kind of text. A press anywhere on it, such as on its icon, puts the cursor in it.
pub fn field(id: impl Into<ElementId>, input: &Entity<InputState>, kind: FieldKind, size: ControlSize) -> Field {
    Field {
        id: id.into(),
        input: input.clone(),
        kind,
        size,
        icon: None,
        after: None,
        label: None,
        off: false,
        grows: false,
    }
}

/// A field to type in, which [`field`] makes.
#[derive(IntoElement)]
pub struct Field {
    id: ElementId,
    input: Entity<InputState>,
    kind: FieldKind,
    size: ControlSize,
    icon: Option<IconName>,
    after: Option<AnyElement>,
    label: Option<SharedString>,
    off: bool,
    grows: bool,
}

impl Field {
    /// An icon before what's typed, in the muted text's colour, such as a search's magnifying glass, as big as a
    /// square's icon of its size.
    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Something at its end, after what's typed, such as the key that finds a setting.
    #[must_use]
    pub fn after(mut self, after: impl IntoElement) -> Self {
        self.after = Some(after.into_any_element());
        self
    }

    /// What an assistive app calls it, where nothing beside it says what it's for.
    #[must_use]
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Greyed out while `off`, so nothing can be typed in it, such as while what was typed is being used.
    #[must_use]
    pub fn off(mut self, off: bool) -> Self {
        self.off = off;
        self
    }

    /// Grows to take the room its row leaves, beside what else is in the row, rather than filling the width it's in.
    #[must_use]
    pub fn grows(mut self) -> Self {
        self.grows = true;
        self
    }
}

impl RenderOnce for Field {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let size = self.size;
        let focused = !self.off && self.input.read(cx).focus_handle(cx).contains_focused(window, cx);
        let gap = match size {
            ControlSize::Small => Space::XSmall,
            ControlSize::Regular | ControlSize::Large => Space::Small,
        };
        let input = Input::new(&self.input)
            .appearance(false)
            .disabled(self.off)
            .when_some(self.label, Input::aria_label)
            .px_0()
            .py_0()
            .h_full()
            .typeset(size.text());
        let text = div().flex_1().min_w_0().h_full().flex().items_center().child(input);
        let icon = self.icon.map(|icon| Icon::new(icon).sized(size.icon()).text_color(colors.text_muted));
        // A press on the icon or what's at the end puts the cursor in it too, as a press on the text does.
        let to_focus = self.input;
        h_flex()
            .id(self.id)
            .test_support()
            .map(|field| if self.grows { field.flex_1().min_w_0() } else { field.w_full() })
            .h(size.height())
            .px(size.padding())
            .gap(gap)
            .when(self.kind == FieldKind::Boxed, |field| {
                field
                    .corners(Corner::Control)
                    .border(EDGE)
                    .border_color(if focused { colors.brand } else { colors.control_edge })
                    .bg(Strength::Strong.of(colors.control_edge))
                    .when(focused, |field| field.focus_ring_style(window, cx))
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                to_focus.update(cx, |input, cx| input.focus(window, cx));
            })
            .children(icon)
            .child(text)
            .children(self.after)
    }
}
