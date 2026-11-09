//! Pairing with a hub: from a link the phone opens, such as the Camera app scanning the hub's QR code, or one
//! pasted in.

use client_core::PairingLink;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, Styled, TestSupportExt as _, Window, div, px,
};

use super::{MobileApp, Pairing, Screen};
use crate::chrome::TAP_HEIGHT;

/// What the hub calls this phone in its list of paired devices. iOS only tells an app the model, not the name
/// its owner gave it.
const DEVICE_NAME: &str = "iPhone";

impl MobileApp {
    /// Pairs with the hub a link names, and follows it once paired. A phone already paired asks first, since the
    /// new hub replaces the old one.
    pub(super) fn pair(&mut self, url: &str, cx: &mut Context<Self>) {
        // A code works once, and the first link is already using it.
        if matches!(self.pairing, Pairing::Busy { .. }) {
            return;
        }
        self.screen = Screen::Pairing;
        match PairingLink::parse(url.trim()) {
            Ok(link) if self.followed.is_some() => self.pairing = Pairing::Confirm(link),
            Ok(link) => self.start_pairing(link, cx),
            Err(e) => self.pairing = Pairing::Failed(format!("That isn't a pairing link: {e}").into()),
        }
        cx.notify();
    }

    fn start_pairing(&mut self, link: PairingLink, cx: &mut Context<Self>) {
        let hub = link.hub_name.clone().into();
        let task = cx.spawn(async move |this, cx| {
            let paired = cx.background_spawn(async move { client_core::pair(&link, DEVICE_NAME) }).await;
            let _ = this.update(cx, |app, cx| {
                match paired {
                    Ok(hub) => {
                        // Still followed for now if it can't be kept, and paired again next launch.
                        app.error = app.vault.save(&hub).err().map(|e| e.to_string().into());
                        app.pairing = Pairing::Idle;
                        app.follow(hub, cx);
                    }
                    Err(e) => app.pairing = Pairing::Failed(format!("Couldn't pair: {e}").into()),
                }
                cx.notify();
            });
        });
        self.pairing = Pairing::Busy { hub, _task: task };
        cx.notify();
    }

    /// Back to the hub this phone has, leaving the link unused.
    fn keep_hub(&mut self, cx: &mut Context<Self>) {
        self.pairing = Pairing::Idle;
        self.show(Screen::Workspaces, cx);
    }

    pub(super) fn pairing_screen(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let insets = window.insets();
        let body = match &self.pairing {
            Pairing::Confirm(link) => self.confirm(link, cx),
            Pairing::Idle | Pairing::Busy { .. } | Pairing::Failed(_) => self.instructions(cx),
        };
        let back = self.followed.as_ref().filter(|_| !matches!(self.pairing, Pairing::Confirm(_))).map(|followed| {
            Button::new("keep-hub")
                .label(format!("Back to {}", followed.link.read(cx).hub().hub_name))
                .ghost()
                .h(px(TAP_HEIGHT))
                .on_click(cx.listener(|app, _, _, cx| app.keep_hub(cx)))
        });
        v_flex()
            .size_full()
            .pt(insets.safe_area.top)
            .pb(insets.effective().bottom + px(16.))
            .px_6()
            .gap_4()
            .justify_center()
            .child(div().flex().justify_center().child(look::brand(px(88.), cx)))
            .child(body)
            .when_some(back, ParentElement::child)
            .into_any_element()
    }

    fn instructions(&self, cx: &mut Context<Self>) -> AnyElement {
        let busy = matches!(self.pairing, Pairing::Busy { .. });
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_4()
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).text_center().child("Pair with a hub"))
            .child(div().text_sm().text_center().text_color(muted).child(
                "On the computer running your hub, run this, then scan the QR code it shows with the Camera app.",
            ))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().muted)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_sm()
                    .text_center()
                    .child("roundhouse phone pair"),
            )
            .child(div().pt_4().text_sm().text_color(muted).child("Or paste the link it prints:"))
            .child(Input::new(&self.link_input).id("pairing-link").disabled(busy))
            .child(
                Button::new("pair")
                    .label(if busy { "Pairing…" } else { "Pair" })
                    .primary()
                    .h(px(TAP_HEIGHT))
                    .disabled(busy)
                    .on_click(cx.listener(|app, _, _, cx| {
                        let url = app.link_input.read(cx).value().to_string();
                        app.pair(&url, cx);
                    })),
            )
            .child(self.pairing_status(cx))
            .into_any_element()
    }

    fn confirm(&self, link: &PairingLink, cx: &mut Context<Self>) -> AnyElement {
        let old =
            self.followed.as_ref().map(|followed| followed.link.read(cx).hub().hub_name.clone()).unwrap_or_default();
        let new = link.hub_name.clone();
        v_flex()
            .gap_4()
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).text_center().child(format!("Pair with {new}?")))
            .child(div().text_sm().text_center().text_color(cx.theme().muted_foreground).child(format!(
                "This phone is paired with {old}. Pairing with {new} replaces it, and reaching {old} again means \
                 pairing with it again."
            )))
            .child(Button::new("confirm-pair").label(format!("Pair with {new}")).primary().h(px(TAP_HEIGHT)).on_click(
                cx.listener(|app, _, _, cx| {
                    if let Pairing::Confirm(link) = std::mem::replace(&mut app.pairing, Pairing::Idle) {
                        app.start_pairing(link, cx);
                    }
                }),
            ))
            .child(
                Button::new("keep-hub")
                    .label(format!("Keep {old}"))
                    .ghost()
                    .h(px(TAP_HEIGHT))
                    .on_click(cx.listener(|app, _, _, cx| app.keep_hub(cx))),
            )
            .into_any_element()
    }

    fn pairing_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, text, color): (_, Option<SharedString>, _) = match &self.pairing {
            Pairing::Idle | Pairing::Confirm(_) => ("pairing-status", None, cx.theme().muted_foreground),
            Pairing::Busy { hub, .. } => {
                ("pairing-status", Some(format!("Pairing with {hub}…").into()), cx.theme().muted_foreground)
            }
            Pairing::Failed(why) => ("pairing-failed", Some(why.clone()), cx.theme().danger),
        };
        div()
            .id(id)
            .min_h(px(40.))
            .text_sm()
            .text_center()
            .text_color(color)
            .when_some(text, ParentElement::child)
            .test_support()
    }
}
