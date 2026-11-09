//! Where a prompt is written: what's attached, what's typed, the model and approval mode pickers, and Send or Stop.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Anchor, Context, InteractiveElement, IntoElement, ParentElement, Styled, Window, div};
use transcript::mode_label;

use super::{WorkspacePane, in_turn};
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

    /// What's attached, what's typed, and below them the attach button, the pickers and Send. Files dropped anywhere on
    /// it are attached, and so is a paste that holds images or files.
    pub(super) fn composer_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pickers = self.pickers(cx).into_any_element();
        let attached = self.attached_tiles(cx);
        let queue = self.queue_card(cx);
        let working = self.session().is_some_and(in_turn);
        let theme = cx.theme();
        let (surface, border) = (theme.muted, theme.input);
        let this = cx.weak_entity();
        let textarea = Textarea::new(&self.composer)
            .appearance(false)
            .on_paste(move |item, _, cx| this.update(cx, |t, cx| t.paste(item, cx)).unwrap_or(false));
        let attach =
            attached::attach_button("attach", cx, |t: &mut Self, paths, cx| t.attach_paths(paths, cx)).xsmall();
        let composer = v_flex().id("composer").w_full().rounded_lg().border_1().border_color(border).bg(surface);
        div().flex_none().w_full().px_4().pb_4().pt_1().children(queue).child(
            attached::takes_drops(composer, cx, |t: &mut Self, paths, cx| t.attach_paths(paths, cx))
                .children(attached)
                .child(div().w_full().px_1().pt_1().child(textarea))
                .child(h_flex().w_full().px_2().pb_2().gap_2().child(attach).child(pickers).child(if working {
                    Button::new("stop")
                        .small()
                        .icon(IconName::Square)
                        .label("Stop")
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
                })),
        )
    }
}
