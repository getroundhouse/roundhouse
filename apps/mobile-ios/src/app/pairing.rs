//! Pairing with a hub: from a link the phone opens, such as the Camera app scanning the hub's QR code, or one pasted
//! in.

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    Styled as _, TestSupportExt as _, Window, div,
};
use look::{AppIconSize, AppIconStyle, ButtonKind, Corner, FieldKind, Shaped as _, Space, Type, Typeset as _};
use protocol::pairing::PairingLink;

use super::{MobileApp, Pairing};
use crate::chrome::TAPPED;

/// What the hub calls this phone in its list of paired devices. iOS only tells an app the model, not the name its
/// owner gave it.
const DEVICE_NAME: &str = "iPhone";

impl MobileApp {
    /// Pairs with the hub a link names, and follows it once paired. A phone already paired asks first, since the new
    /// hub replaces the old one.
    pub(super) fn pair(&mut self, url: &str, cx: &mut Context<Self>) {
        // A code works once, and the first link is already using it.
        if matches!(self.pairing, Pairing::Busy { .. }) {
            return;
        }
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
        cx.notify();
    }

    pub(super) fn pairing_screen(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let insets = window.insets();
        let body = match &self.pairing {
            Pairing::Confirm(link) => self.confirm(link, cx),
            Pairing::Idle | Pairing::Busy { .. } | Pairing::Failed(_) => self.instructions(cx),
        };
        let back = self.followed.as_ref().filter(|_| !matches!(self.pairing, Pairing::Confirm(_))).map(|followed| {
            let words = format!("Back to {}", followed.link.read(cx).hub().hub_name);
            look::button("keep-hub", ButtonKind::Borderless, TAPPED, words)
                .full_width()
                .on_click(cx.listener(|app, _, _, cx| app.keep_hub(cx)))
        });
        v_flex()
            .size_full()
            .pt(insets.safe_area.top)
            .pb(insets.effective().bottom + Space::Large.size())
            .px(Space::XLarge)
            .gap(Space::Large)
            .justify_center()
            .child(div().flex().justify_center().child(look::app_icon(AppIconStyle::Idle, AppIconSize::Px128, cx)))
            .child(body)
            .children(back)
            .into_any_element()
    }

    fn instructions(&self, cx: &mut Context<Self>) -> AnyElement {
        let busy = matches!(self.pairing, Pairing::Busy { .. });
        let colors = look::colors(cx);
        let pair = look::button("pair", ButtonKind::Main, TAPPED, if busy { "Pairing…" } else { "Pair" })
            .full_width()
            .waiting(busy)
            .on_click(cx.listener(|app, _, _, cx| {
                let url = app.link_input.read(cx).value().to_string();
                app.pair(&url, cx);
            }));
        let command = div()
            .px(Space::Medium)
            .py(Space::Small)
            .corners(Corner::Control)
            .bg(colors.inset)
            .typeset(Type::Code)
            .text_center()
            .child("roundhouse phone pair");
        v_flex()
            .gap(Space::Large)
            .child(div().typeset(Type::PageTitle).text_center().child("Pair with a hub"))
            .child(div().typeset(Type::Body).text_center().text_color(colors.text_muted).child(
                "On the computer running your hub, run this, then scan the QR code it shows with the Camera app.",
            ))
            .child(command)
            .child(
                div()
                    .pt(Space::Large)
                    .typeset(Type::Body)
                    .text_color(colors.text_muted)
                    .child("Or paste the link it prints:"),
            )
            .child(look::field("pairing-link", &self.link_input, FieldKind::Boxed, TAPPED).off(busy))
            .child(pair)
            .child(self.pairing_status(cx))
            .into_any_element()
    }

    fn confirm(&self, link: &PairingLink, cx: &mut Context<Self>) -> AnyElement {
        let old = self.followed.as_ref().map_or_default(|followed| followed.link.read(cx).hub().hub_name.clone());
        let new = link.hub_name.clone();
        let muted = look::colors(cx).text_muted;
        let confirm = look::button("confirm-pair", ButtonKind::Main, TAPPED, format!("Pair with {new}"))
            .full_width()
            .on_click(cx.listener(|app, _, _, cx| {
                if let Pairing::Confirm(link) = std::mem::replace(&mut app.pairing, Pairing::Idle) {
                    app.start_pairing(link, cx);
                }
            }));
        let keep = look::button("keep-hub", ButtonKind::Borderless, TAPPED, format!("Keep {old}"))
            .full_width()
            .on_click(cx.listener(|app, _, _, cx| app.keep_hub(cx)));
        v_flex()
            .gap(Space::Large)
            .child(div().typeset(Type::PageTitle).text_center().child(format!("Pair with {new}?")))
            .child(div().typeset(Type::Body).text_center().text_color(muted).child(format!(
                "This phone is paired with {old}. Pairing with {new} replaces it, and reaching {old} again means \
                 pairing with it again."
            )))
            .child(confirm)
            .child(keep)
            .into_any_element()
    }

    fn pairing_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let (id, text, color): (_, Option<SharedString>, _) = match &self.pairing {
            Pairing::Idle | Pairing::Confirm(_) => ("pairing-status", None, colors.text_muted),
            Pairing::Busy { hub, .. } => {
                ("pairing-status", Some(format!("Pairing with {hub}…").into()), colors.text_muted)
            }
            Pairing::Failed(why) => ("pairing-failed", Some(why.clone()), colors.error),
        };
        div()
            .id(id)
            .min_h(look::RowSize::Regular.height())
            .typeset(Type::Detail)
            .text_center()
            .text_color(color)
            .when_some(text, ParentElement::child)
            .test_support()
    }
}
