//! The paired hub: how the connection stands, where the phone looks for it, and forgetting it.

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use look::{ButtonKind, Space, Type, Typeset as _};

use super::{Followed, MobileApp};
use crate::chrome;
use crate::hub_link::LinkState;

impl MobileApp {
    pub(super) fn hub_screen(&self, followed: &Followed, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let back = chrome::back().on_click(cx.listener(|app, _, _, cx| app.show_panel(None, cx)));
        let link = followed.link.read(cx);
        let hub = link.paired();
        let why = match link.state() {
            LinkState::Unreachable(why) | LinkState::HubOlder(why) | LinkState::Refused(why) => Some(why.clone()),
            LinkState::Connecting | LinkState::Ready => None,
        };
        let muted = look::colors(cx).text_muted;
        let heading =
            |text: &'static str| div().pt(Space::Large).typeset(Type::Detail).emphasis().text_color(muted).child(text);
        let detail = |text: String| div().typeset(Type::Detail).text_color(muted).child(text);
        let forget = look::button("forget", ButtonKind::Removes, chrome::TAPPED, "Forget this hub")
            .full_width()
            .on_click(cx.listener(|app, _, _, cx| app.forget(cx)));
        let header = chrome::header(hub.hub_name.clone().into(), None, Some(back), Vec::new(), window, cx);
        let error = self.error.clone().map(chrome::error);
        let body = v_flex()
            .flex_1()
            .px(Space::Large)
            .pb(crate::safe_area::insets(window, cx).safe_area.bottom + Space::Large.size())
            .child(heading("Connection"))
            .child(div().typeset(Type::Body).child(chrome::connection(link.state())))
            .when_some(why, |d, why| d.child(detail(why)))
            .child(heading("Looks for it at"))
            .child(div().typeset_code(cx).child(hub.hosts.join("\n")))
            .child(detail(
                "The phone needs the hub's network, or a VPN to it such as Tailscale, and the hub's computer awake."
                    .into(),
            ))
            .child(div().flex_1())
            .child(forget);
        v_flex().size_full().child(header).children(error).child(body).into_any_element()
    }
}
