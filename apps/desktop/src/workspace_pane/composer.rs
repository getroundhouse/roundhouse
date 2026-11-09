//! Where a prompt is written: what's attached, what's typed, the model and approval mode pickers, and Send or Stop.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Anchor, Context, ExternalPaths, InteractiveElement, IntoElement, ParentElement, Styled, Window, div};
use transcript::mode_label;

use super::{WorkspacePane, in_turn};
use crate::StopAgent;

/// Aliases, so these stay the newest of each without a release here. `default` is the account's own.
pub(super) const MODELS: [(&str, &str); 5] =
    [("default", "Default model"), ("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];

const MODES: [&str; 4] = ["manual", "plan", "acceptEdits", "bypassPermissions"];

pub(super) fn model_label(model: Option<&str>) -> String {
    let model = model.unwrap_or("default");
    MODELS.iter().find(|(id, _)| *id == model).map_or(model, |(_, label)| label).to_string()
}

impl WorkspacePane {
    /// The model and approval mode of the chat showing.
    fn pickers(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let model = session.and_then(|s| s.model.clone());
        let mode = session.and_then(|s| s.permission_mode.clone());
        let full_access = mode.as_deref() == Some("bypassPermissions");
        let warning = cx.theme().warning;
        let mode_text = mode.as_deref().map_or("Your settings", mode_label).to_string();
        let this = cx.weak_entity();
        let models = {
            let (this, current) = (this.clone(), model.clone().unwrap_or_else(|| "default".to_string()));
            move |mut menu: gpui_kit::component::menu::PopupMenu, _: &mut Window, _: &mut Context<_>| {
                for (id, label) in MODELS {
                    let this = this.clone();
                    menu = menu.item(PopupMenuItem::new(label).checked(current == id).on_click(move |_, _, cx| {
                        let _ = this.update(cx, |t, cx| t.configure(Some(id.to_string()), None, cx));
                    }));
                }
                menu
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
                            let _ = this.update(cx, |t, cx| t.configure(None, Some(id.to_string()), cx));
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
            .child(
                Button::new("model-picker")
                    .ghost()
                    .xsmall()
                    .dropdown_caret(true)
                    .label(model_label(model.as_deref()))
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, models),
            )
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
    /// it are attached, and so is a paste that holds images or files. It's centred under the transcript's column and a
    /// little wider, so the transcript's text sits inside its edges.
    pub(super) fn composer_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pickers = self.pickers(cx).into_any_element();
        let attached = self.attached_tiles(cx);
        let queue = self.queue_card(cx);
        let working = self.session().is_some_and(in_turn);
        let theme = cx.theme();
        let (surface, border, ring) = (theme.muted, theme.input, theme.ring);
        let this = cx.weak_entity();
        let textarea = Textarea::new(&self.composer)
            .appearance(false)
            .on_paste(move |item, _, cx| this.update(cx, |t, cx| t.paste(item, cx)).unwrap_or(false));
        let attach = Button::new("attach")
            .ghost()
            .xsmall()
            .icon(IconName::Plus)
            .tooltip("Attach images or files")
            .on_click(cx.listener(|_, _, _, cx| Self::pick_files(cx)));
        let composer = v_flex()
            .id("composer")
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(border)
            .bg(surface)
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(ring))
            .on_drop(cx.listener(|t, dropped: &ExternalPaths, _, cx| t.attach_paths(dropped.paths().to_vec(), cx)))
            .children(attached)
            .child(div().w_full().px_2().pt_2().child(textarea))
            .child(h_flex().w_full().px_3().pt_2().pb_3().gap_2().child(attach).child(pickers).child(if working {
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
            }));
        transcript::around_column(v_flex().w_full().children(queue).child(composer)).flex_none().pb_4().pt_1()
    }
}
