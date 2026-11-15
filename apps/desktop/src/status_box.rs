//! A box that says how something stands, with an icon, a line of words, how far it has got, and what can be done
//! about it, as a button. Settings draws one under a setting, and the composer's microphone opens one. It's a status,
//! so a screen reader says it as it changes.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::Button;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, HighlightStyle, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledText, TestSupportExt as _, div,
};
use look::{Hand as _, IconSizing as _, Off as _, Shaped as _, Sizing as _, Typeset as _};

/// How something stands, to be drawn as a box.
pub(crate) struct StatusBox {
    /// The box's, for tests and a screen reader to find it by.
    pub(crate) id: &'static str,
    pub(crate) icon: Lucide,
    pub(crate) tone: Tone,
    /// Said first, in the text's colour, such as "Ready.".
    pub(crate) lead: Option<SharedString>,
    /// The rest, quieter.
    pub(crate) detail: SharedString,
    /// How far it has got, from 0 to 1, as a bar under the words.
    pub(crate) progress: Option<f32>,
    pub(crate) action: Option<Action>,
}

/// What the icon's colour says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    /// Ready, or done.
    Done,
    /// Under way.
    Busy,
    /// Something went wrong, or can't be done here.
    Problem,
    /// Nothing's under way, and something's still kept.
    Quiet,
}

/// A button in the box.
pub(crate) struct Action {
    /// The button's, for tests to find it by.
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    /// It can't be pressed just now.
    pub(crate) off: bool,
    pub(crate) act: fn(&mut App),
}

impl StatusBox {
    /// What a screen reader says of it: its words, as they read.
    fn said(&self) -> String {
        match &self.lead {
            Some(lead) if self.detail.is_empty() => lead.to_string(),
            Some(lead) => format!("{lead} {}", self.detail),
            None => self.detail.to_string(),
        }
    }

    pub(crate) fn draw(self, cx: &App) -> AnyElement {
        let colors = look::colors(cx);
        let icon_color = match self.tone {
            Tone::Done => colors.done,
            Tone::Busy => colors.text,
            Tone::Problem => colors.warning,
            Tone::Quiet => colors.text_muted,
        };
        let said = self.said();
        // One paragraph, so the detail wraps on from the lead, which only its colour sets apart.
        let lead = self.lead.as_ref().map_or(0, |lead| lead.len());
        let lead =
            (lead > 0).then(|| (0..lead, HighlightStyle { color: Some(colors.text), ..HighlightStyle::default() }));
        let words = StyledText::new(SharedString::from(said.clone())).with_highlights(lead);
        let line = h_flex()
            .w_full()
            .gap(look::Space::Small)
            .child(Icon::new(self.icon).sized(look::IconSize::Regular).text_color(icon_color).flex_none())
            .child(div().flex_1().min_w_0().text_color(colors.text_muted).child(words));
        let action = self.action.map(|action| {
            let act = action.act;
            Button::new(action.id)
                .hand()
                .outline()
                .sized(look::ControlSize::Small)
                .label(action.label)
                .off_while(action.off)
                .on_click(move |_, _, cx| act(cx))
        });
        let bar = self.progress.map(|done| {
            let bar = Progress::new(format!("{}-progress", self.id)).value(done * 100.).color(colors.text);
            bar.accessibility_label(said.clone())
        });
        v_flex()
            .id(self.id)
            .role(Role::Status)
            .aria_label(said)
            .test_support()
            .w_full()
            .min_h_11()
            .justify_center()
            .gap(look::Space::Small)
            // Less at the end, where a button stands in the room.
            .pl(look::Space::Medium)
            .pr(look::Space::Small)
            .py(look::Space::Small)
            .corners(look::Corner::Card)
            .border_1()
            .border_color(colors.edge)
            .bg(colors.inset)
            .typeset(look::Type::Body)
            .child(h_flex().w_full().gap(look::Space::Medium).child(line).children(action))
            .children(bar)
            .into_any_element()
    }
}
