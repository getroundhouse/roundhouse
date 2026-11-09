//! The paired hub: where the phone looks for it, how the connection stands, and forgetting it.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, Window, div, px};

use super::{Followed, MobileApp, Screen};
use crate::chrome::{self, TAP_HEIGHT};
use crate::hub_link::LinkState;

impl MobileApp {
    pub(super) fn hub_info_screen(followed: &Followed, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let back = chrome::back_button().on_click(cx.listener(|app, _, _, cx| app.show(Screen::Workspaces, cx)));
        let link = followed.link.read(cx);
        let hub = link.hub();
        let why = match link.state() {
            LinkState::Unreachable(why) => Some(why.clone()),
            LinkState::Connecting | LinkState::Ready => None,
        };
        let muted = cx.theme().muted_foreground;
        let label =
            |text: &'static str| div().pt_4().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(muted).child(text);
        let header = chrome::header(hub.hub_name.clone().into(), None, Some(back.into_any_element()), None, window, cx);
        v_flex()
            .size_full()
            .child(header)
            .child(
                v_flex()
                    .flex_1()
                    .px_4()
                    .pb(window.insets().safe_area.bottom + px(16.))
                    .child(label("CONNECTION"))
                    .child(div().text_sm().child(chrome::connection(link.state())))
                    .when_some(why, |this, why| this.child(div().text_xs().text_color(muted).child(why)))
                    .child(label("LOOKS FOR IT AT"))
                    .child(div().text_sm().font_family(cx.theme().mono_font_family.clone()).child(hub.hosts.join("\n")))
                    .child(div().text_xs().pt_2().text_color(muted).child(
                        "The phone needs the same network as the hub, or a VPN to it such as Tailscale, and the hub's computer awake.",
                    ))
                    .child(div().flex_1())
                    .child(
                        Button::new("forget")
                            .label("Forget this hub")
                            .danger()
                            .h(px(TAP_HEIGHT))
                            .on_click(cx.listener(|app, _, _, cx| app.forget(cx))),
                    ),
            )
            .into_any_element()
    }
}
