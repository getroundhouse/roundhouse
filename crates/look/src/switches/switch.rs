use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{App, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, Window};

use super::SwitchKind;
use crate::{Button, ButtonKind, ControlSize, Corner, Shaped as _, Space, button, colors, icon_button};

/// A switch of `kind`, with no segments yet, which [`Switch::words`] and [`Switch::icon`] add, in order.
pub fn switch(kind: SwitchKind) -> Switch {
    Switch { kind, segments: Vec::new() }
}

/// A choice of one of a few, which [`switch`] makes.
#[derive(IntoElement)]
pub struct Switch {
    kind: SwitchKind,
    segments: Vec<Button>,
}

impl Switch {
    /// The size of its segments: small on a track, so the track is as tall as a regular button, and regular without.
    fn segment_size(&self) -> ControlSize {
        match self.kind {
            SwitchKind::Tracked => ControlSize::Small,
            SwitchKind::Trackless => ControlSize::Regular,
        }
    }

    /// A segment that says `words`, found by `id`, on while `chosen`, which does `choose` as it's pressed.
    #[must_use]
    pub fn words(
        mut self,
        id: impl Into<ElementId>,
        words: impl Into<SharedString>,
        chosen: bool,
        choose: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let segment = button(id, ButtonKind::Borderless, self.segment_size(), words);
        self.segments.push(segment.on(chosen).on_click(move |_, window, cx| choose(window, cx)));
        self
    }

    /// A segment that's only `icon`, found by `id`, that says what it is, `called`, while the pointer rests on it and
    /// to assistive apps, on while `chosen`, which does `choose` as it's pressed.
    #[must_use]
    pub fn icon(
        mut self,
        id: impl Into<ElementId>,
        icon: impl Into<Icon>,
        called: &'static str,
        chosen: bool,
        choose: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let segment = icon_button(id, ButtonKind::Borderless, self.segment_size(), icon).tooltip(called);
        self.segments.push(segment.on(chosen).on_click(move |_, window, cx| choose(window, cx)));
        self
    }
}

impl Switch {
    /// What the segment added last does, while the pointer rests on it, such as a key that does the same.
    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        if let Some(segment) = self.segments.pop() {
            self.segments.push(segment.tooltip(tooltip));
        }
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let switch = h_flex().flex_none();
        match self.kind {
            SwitchKind::Tracked => {
                // The room that makes the track as tall as a regular button, around and between its small segments.
                let room = (ControlSize::Regular.height() - ControlSize::Small.height()) / 2.;
                switch.p(room).gap(room).corners(Corner::Control).bg(colors(cx).raised).children(self.segments)
            }
            SwitchKind::Trackless => switch.gap(Space::XSmall).children(self.segments),
        }
    }
}
