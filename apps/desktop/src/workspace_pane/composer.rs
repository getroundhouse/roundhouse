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
use look::{Hand as _, Off as _};
use transcript::mode_label;

use super::WorkspacePane;
use crate::StopAgent;
use crate::attached;
use crate::model_picker::{Picked, model_picker};

const MODES: [&str; 4] = ["manual", "plan", "acceptEdits", "bypassPermissions"];

/// Whether there's a hub to send what's typed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hub {
    Here,
    /// The app is connecting to one again, with the pane still on screen.
    Away,
}

impl WorkspacePane {
    /// For when the app starts connecting to a hub again, with this still on screen, so what's typed meanwhile isn't
    /// lost. Until [`Self::reconnect`], nothing typed is sent, and it stays in the composer: the hub may be one that's
    /// being stopped, which would take a prompt and then cut its agent off.
    pub fn hub_went_away(&mut self, cx: &mut Context<Self>) {
        self.hub = Hub::Away;
        cx.notify();
    }

    /// Sends what's typed, as Enter and Send do, unless the hub is away, when it stays typed for when it's back.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hub == Hub::Away {
            return;
        }
        let text = self.composer.read(cx).text().to_string();
        let Some(transcript) = self.transcript() else { return };
        // While the agent is waiting on a question, what's typed is the answer to it, and the files wait for the next
        // prompt, since an answer can't carry them.
        if transcript.update(cx, |t, cx| t.answer_with_text(&text, cx)) {
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
            return;
        }
        let files = self.attached.take();
        if transcript.update(cx, |t, cx| t.send_prompt_with(text, files, cx)) {
            self.composer.update(cx, |c, cx| c.set_value("", window, cx));
        }
        cx.notify();
    }

    /// The model and its effort, and the approval mode, of the chat showing.
    fn pickers(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let model_and_effort = session.map(|s| (s.model.clone(), s.effort));
        let mode = session.and_then(|s| s.permission_mode.clone());
        let full_access = mode.as_deref() == Some("bypassPermissions");
        let warning = cx.theme().warning;
        let mode_text = mode.as_deref().map_or("Your settings", mode_label).to_string();
        let this = cx.weak_entity();
        let models = {
            let this = this.clone();
            move |picked, _: &mut Window, cx: &mut gpui_kit::App| {
                let _ = this.update(cx, |t, cx| match picked {
                    Picked::Model(model) => t.configure(Some(model), None, None, cx),
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
            // Only a chat has a model, so there's none to show while the workspace's first is being made.
            .when_some(model_and_effort, |row, (model, effort)| {
                row.child(model_picker("model-picker", &model, effort, models))
            })
            .child(
                Button::new("mode-picker")
                    .hand()
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
    /// stands apart.
    fn bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let attach =
            attached::attach_button("attach", cx, |t: &mut Self, paths, cx| t.attach_paths(paths, cx)).xsmall();
        let pickers = self.pickers(cx).into_any_element();
        let beside = self.beside_the_main_action(cx);
        // Left out when there's nothing in it, so it doesn't push the main action in by a gap.
        let beside = (!beside.is_empty()).then(|| h_flex().flex_none().gap_2().children(beside));
        let agents = h_flex().flex_none().gap_4().children(beside).child(self.main_action(cx));
        h_flex().w_full().px_3().pt_2().pb_3().gap_2().child(attach).child(pickers).child(agents)
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
                .hand()
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
                // What's typed waits for the hub, as `hub_went_away` says.
                .off_while(self.hub == Hub::Away)
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
