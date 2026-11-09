//! Pairing with a hub: from a link the phone opens, such as the Camera app scanning the hub's QR code, or one
//! pasted in.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, FontWeight, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};

use super::{MobileApp, Pairing};
use crate::chrome::TAP_HEIGHT;
use client_core::PairingLink;

/// What the hub calls this phone in its list of paired devices. iOS only tells an app the model, not the name
/// its owner gave it.
const DEVICE_NAME: &str = "iPhone";

impl MobileApp {
    /// Pairs with the hub a link names, and follows it once paired, in place of any hub followed before.
    pub(super) fn pair(&mut self, url: &str, cx: &mut Context<Self>) {
        let link = match PairingLink::parse(url.trim()) {
            Ok(link) => link,
            Err(e) => {
                self.pairing = Pairing::Failed(format!("That isn't a pairing link: {e}").into());
                cx.notify();
                return;
            }
        };
        self.pairing = Pairing::Busy(link.hub_name.clone().into());
        cx.notify();
        let pair = self.services.pair.clone();
        self.working = Some(cx.spawn(async move |this, cx| {
            let paired = cx.background_spawn(async move { pair(&link, DEVICE_NAME) }).await;
            let _ = this.update(cx, |app, cx| {
                app.working = None;
                match paired {
                    Ok(hub) => {
                        // Still followed for now if it can't be kept, and paired again next launch.
                        app.error = app.services.vault.save(&hub).err().map(|e| e.to_string().into());
                        app.pairing = Pairing::Idle;
                        app.follow(hub, cx);
                    }
                    Err(e) => app.pairing = Pairing::Failed(format!("Couldn't pair: {e}").into()),
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn pairing_screen(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let insets = window.insets();
        let busy = matches!(self.pairing, Pairing::Busy(_));
        let muted = cx.theme().muted_foreground;
        v_flex()
            .size_full()
            .pt(insets.safe_area.top)
            .pb(insets.effective().bottom + px(16.))
            .px_6()
            .gap_4()
            .justify_center()
            .child(div().flex().justify_center().child(look::brand(px(88.), cx)))
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
    }

    fn pairing_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, color): (Option<SharedString>, _) = match &self.pairing {
            Pairing::Idle => (None, cx.theme().muted_foreground),
            Pairing::Busy(hub) => (Some(format!("Pairing with {hub}…").into()), cx.theme().muted_foreground),
            Pairing::Failed(why) => (Some(why.clone()), cx.theme().danger),
        };
        div().min_h(px(40.)).text_sm().text_center().text_color(color).when_some(text, ParentElement::child)
    }
}
