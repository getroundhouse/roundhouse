//! The paired hub: where the phone looks for it, how the connection stands, and forgetting it.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, IconName, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement, SharedString, Styled, Window, div, px};

use super::{MobileApp, Screen};
use crate::chrome::{self, TAP_HEIGHT};
use crate::hub_link::LinkState;

impl MobileApp {
    pub(super) fn hub_info_screen(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let insets = window.insets();
        let back = Button::new("back")
            .icon(IconName::ChevronLeft)
            .ghost()
            .w(px(TAP_HEIGHT))
            .h(px(TAP_HEIGHT))
            .on_click(cx.listener(|app, _, _, cx| app.show(Screen::Workspaces, cx)));
        let state = self.link().map(|link| link.read(cx).state().clone());
        let hub = self.followed.as_ref().map(|followed| &followed.hub);
        let status: SharedString = match &state {
            Some(LinkState::Connecting) => "Connecting…".into(),
            Some(LinkState::Ready) => "Connected".into(),
            Some(LinkState::Unreachable(why)) => format!("Can't reach it: {why}").into(),
            None => "Not paired".into(),
        };
        let hosts = hub.map(|hub| hub.hosts.join("\n")).unwrap_or_default();
        let name: SharedString = hub.map_or_else(|| "Hub".into(), |hub| hub.hub_name.clone().into());
        let muted = cx.theme().muted_foreground;
        let label =
            |text: &'static str| div().pt_4().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(muted).child(text);
        v_flex()
            .size_full()
            .child(chrome::header(name, None, Some(back.into_any_element()), None, insets.safe_area.top, cx))
            .child(
                v_flex()
                    .flex_1()
                    .px_4()
                    .pb(insets.safe_area.bottom + px(16.))
                    .child(label("CONNECTION"))
                    .child(div().text_sm().child(status))
                    .child(label("LOOKS FOR IT AT"))
                    .child(div().text_sm().font_family(cx.theme().mono_font_family.clone()).child(hosts))
                    .child(div().text_xs().pt_2().text_color(muted).child(
                        "The phone needs the same network as the hub, or a VPN to it such as Tailscale, and the hub's computer awake.",
                    ))
                    .child(div().flex_1())
                    .when(hub.is_some(), |this| {
                        this.child(
                            Button::new("forget")
                                .label("Forget this hub")
                                .danger()
                                .h(px(TAP_HEIGHT))
                                .on_click(cx.listener(|app, _, _, cx| app.forget(cx))),
                        )
                    }),
            )
    }
}
