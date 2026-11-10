//! Where a prompt is written: what's attached, what's typed, and the bar under them, with the model and approval mode
//! pickers at one end, and at the other what the agent says of itself, then Send or Stop.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, Pixels, Styled, Window, div, px,
};
use transcript::mode_label;

use super::WorkspacePane;
use crate::StopAgent;
use crate::attached;
use crate::model_picker::{Picked, model_picker};

const MODES: [&str; 4] = ["manual", "plan", "acceptEdits", "bypassPermissions"];

impl WorkspacePane {
    /// The model and its effort, and the approval mode, of the chat showing.
    fn pickers(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let model = session.and_then(|s| s.model.clone());
        let effort = session.and_then(|s| s.effort);
        let mode = session.and_then(|s| s.permission_mode.clone());
        let full_access = mode.as_deref() == Some("bypassPermissions");
        let warning = cx.theme().warning;
        let mode_text = mode.as_deref().map_or("Your settings", mode_label).to_string();
        let this = cx.weak_entity();
        let models = {
            let this = this.clone();
            move |picked, _: &mut Window, cx: &mut gpui_kit::App| {
                let _ = this.update(cx, |t, cx| match picked {
                    Picked::Model(id) => t.configure(Some(id.to_string()), None, None, cx),
                    Picked::Effort(effort) => t.configure(None, None, Some(effort), cx),
                });
            }
        };
        let approval_modes = {
            let current = mode.clone();
            move |mut menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                for id in MODES {
                    let this = this.clone();
                    let checked = current.as_deref() == Some(id);
                    menu = menu.item(PopupMenuItem::new(mode_label(id).to_string()).checked(checked).on_click(
                        move |_, _, cx| {
                            let _ = this.update(cx, |t, cx| t.configure(None, Some(id.to_string()), None, cx));
                        },
                    ));
                }
                menu
            }
        };
        h_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(model_picker("model-picker", model.as_deref(), effort, models))
            .child(
                Button::new("mode-picker")
                    .ghost()
                    .xsmall()
                    .dropdown_caret(true)
                    .when(full_access, |b| b.text_color(warning))
                    .label(mode_text)
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, approval_modes),
            )
    }

    /// What's attached, what's typed, and below them the bar. Files dropped anywhere on it are attached, and so is a
    /// paste that holds images or files. It's centred under the transcript's column and a little wider, so the
    /// transcript's text sits inside its edges.
    pub(super) fn composer_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let attached = self.attached_tiles(cx);
        let queue = self.queue_card(cx);
        let bar = self.bar(cx).into_any_element();
        let theme = cx.theme();
        let (surface, border) = (theme.muted, theme.input);
        let this = cx.weak_entity();
        let textarea = Textarea::new(&self.composer)
            .appearance(false)
            .on_paste(move |item, _, cx| this.update(cx, |t, cx| t.paste(item, cx)).unwrap_or(false));
        let composer = v_flex().id("composer").w_full().rounded_lg().border_1().border_color(border).bg(surface);
        let composer = attached::takes_drops(composer, cx, |t: &mut Self, paths, cx| t.attach_paths(paths, cx))
            .children(attached)
            .child(div().w_full().px_2().pt_2().child(textarea))
            .child(bar);
        transcript::around_column(v_flex().w_full().children(queue).child(composer)).flex_none().pb_4().pt_1()
    }

    /// The row along the composer's bottom, which has two ends. The left one sets up the prompt: the attach button,
    /// then the pickers, which take whatever room is left. The right one is the agent's: what it says of itself,
    /// each in a [`Slot`], and last the one main action, twice as far from them as they are from each other, so it
    /// stands a little apart.
    fn bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let attach =
            attached::attach_button("attach", cx, |t: &mut Self, paths, cx| t.attach_paths(paths, cx)).xsmall();
        let pickers = self.pickers(cx).into_any_element();
        let beside = self.beside_the_main_action(cx);
        // Left out when there's nothing in it, so it doesn't push the main action in by a gap.
        let beside = (!beside.is_empty()).then(|| h_flex().flex_none().gap_1().children(beside));
        h_flex()
            .w_full()
            .px_3()
            .pt_2()
            .pb_3()
            .gap_2()
            .child(attach)
            .child(pickers)
            .children(beside)
            .child(self.main_action(cx))
    }

    /// What sits beside the main action, in the order it's drawn. Something new for the bar's right end goes here,
    /// and being a [`Slot`] is what lines it up and spaces it.
    fn beside_the_main_action(&self, cx: &mut Context<Self>) -> Vec<Slot> {
        self.context_ring(cx).into_iter().collect()
    }

    /// Send, or Stop while the agent is in a turn. Work left in the background isn't one, since stopping a turn
    /// doesn't reach it.
    fn main_action(&self, cx: &mut Context<Self>) -> AnyElement {
        let working = self.session().is_some_and(|s| s.status.in_turn());
        if working {
            let stop_mark = cx.theme().button_primary_foreground;
            // The same square as Send, with a filled square rather than an icon, which would only be outlined.
            Button::new("stop")
                .primary()
                .small()
                .size_6()
                .p_0()
                .child(div().size_2p5().rounded_xs().bg(stop_mark))
                .accessibility_label("Stop")
                .tooltip_with_action("Stop", &StopAgent, None)
                .on_click(cx.listener(|t, _, _, cx| t.stop_agent(cx)))
                .into_any_element()
        } else {
            Button::new("send")
                .primary()
                .small()
                .icon(IconName::ArrowUp)
                .tooltip("Send")
                .on_click(cx.listener(|t, _, window, cx| t.submit(window, cx)))
                .into_any_element()
        }
    }
}

/// A place beside the main action: a square as big as it, with what it holds in its middle, or filling it if it's
/// made to. So things of different sizes line up with the main action, sit the same distance apart, and are as easy
/// to point at as each other. The bar takes only these, so nothing is put there any other way.
pub(super) struct Slot(Div);

impl Slot {
    /// How wide and tall a slot is, which is the main action's size: a small control's, in `docs/spec/look.md`.
    pub(super) const SIZE: Pixels = px(24.);

    pub(super) fn holding(holds: impl IntoElement) -> Self {
        Self(middle().flex_none().size(Self::SIZE).child(holds))
    }
}

impl IntoElement for Slot {
    type Element = Div;

    fn into_element(self) -> Div {
        self.0
    }
}

/// Something with whatever it's given in its middle.
pub(super) fn middle() -> Div {
    div().flex().items_center().justify_center()
}
