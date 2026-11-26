//! The plug beside Send, and its card, which lists the MCP servers the chat has: the hub's, then the ones its agent's CLI
//! has of its own. What's said of each, and whether a reload is offered, is `client_core::tool_servers`'s, and signing in
//! to one is the hub's view's, in `crate::tool_sign_in`, so every chat's card says the same of it.

use client_core::tool_servers::{
    ChatToolServer, ChatToolServers, Offered, OwnToolServers, Reload, ReloadRequest, Tone,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use look::{CardKind, IconSizing as _, Lined as _, Region, Typeset as _};
use protocol::capability::setup::ServerName;
use protocol::model::ToolServerId;

use super::super::composer::{Hub, Slot, middle};
use super::super::{PaneEvent, WorkspacePane};
use super::card::surface;
use super::usage::card_over;
use crate::tool_sign_in::{SignInTo, UnderWay};

/// How tall the card's servers get before they scroll, so a long list stays in the window: about ten rows.
const MOST_HEIGHT: Pixels = px(360.);

impl WorkspacePane {
    /// The plug for the chat showing, while there's something to say of its servers. It's as quiet as the ring, or amber
    /// while one of the hub's needs the person to sign in to it. It's one of Tab's stops, since its card has buttons.
    pub(in super::super) fn plug(&self, cx: &mut Context<Self>) -> Option<Slot> {
        let sign_in = self.tool_sign_in.read(cx).under_way().and_then(UnderWay::of_a_server);
        let servers = ChatToolServers::of(self.shown.all(), self.session()?, sign_in.as_ref(), self.reload_request)?;
        let colors = look::colors(cx);
        let color = if servers.needs_sign_in() { colors.warning } else { colors.text_muted };
        let icon = Icon::new(IconName::Plug).sized(look::IconSize::Large).text_color(color);
        // The whole slot is the plug's to point at, as it is the ring's. A press on it leaves the focus in the composer.
        let plug = middle()
            .size(Slot::SIZE.height())
            .child(icon)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default());
        // Said in words too, since a colour alone says nothing to someone who can't see it.
        let said = servers.said();
        let heading = Heading {
            own: match self.agent() {
                Some(agent) => format!("{}'s own", agent.name).into(),
                None => "The agent's own".into(),
            },
            machine: crate::names::runs_on(self.workspace()),
        };
        let offers = Offers { pane: cx.weak_entity(), off: self.hub == Hub::Away };
        // With a button in it, it waits for the pointer to cross to it.
        let kind = if servers.offers() { look::hover_card_with_a_button } else { look::hover_card };
        let hover = card_over(kind("mcp-hover"), plug)
            .on_focus(Role::Group, said.clone(), servers.each_said())
            .content(move |_, _, cx| card(&servers, heading, &offers, cx));
        Some(Slot::holding(div().id("mcp").role(Role::Status).aria_label(said).child(hover).test_support()))
    }

    /// Signs in to the hub's `server` again, for every chat, if the hub has it still.
    fn sign_in_to(&mut self, server: &ToolServerId, cx: &mut Context<Self>) {
        if let Some(server) = self.shown.all().tool_server(server) {
            let again = SignInTo::again(server);
            self.tool_sign_in.update(cx, |signing_in, cx| signing_in.start(again, cx));
        }
    }

    /// Has the chat's agent connect again to the server it has as `server`. The row says how that goes, as the agent says
    /// it's starting and then what came of it, and what the hub refuses is said as the pane's other refusals are.
    fn reload_tool_server(&mut self, server: ServerName, cx: &mut Context<Self>) {
        let Some(session) = self.session().map(|session| session.id.clone()) else { return };
        let asking = self.client.start(move |client| client.session_reload_tool_server(&session, &server));
        cx.spawn(async move |this, cx| {
            let Err(e) = asking.recv().await else { return };
            let _ = this.update(cx, |pane, cx| {
                if e.is_unsupported() {
                    pane.reload_request = ReloadRequest::UnknownToTheHub;
                    cx.notify();
                }
                cx.emit(PaneEvent::Error(client_core::refusals::couldnt("reload it", &e)));
            });
        })
        .detach();
    }
}

/// What the rows' buttons do what they do through.
struct Offers {
    pane: WeakEntity<WorkspacePane>,
    /// With no hub to ask.
    off: bool,
}

impl Offers {
    /// The button that signs in to `server`, which the person calls `name`.
    fn sign_in(&self, server: &ToolServerId, name: &str) -> look::Button {
        let (pane, to) = (self.pane.clone(), server.clone());
        let id = SharedString::from(format!("mcp-sign-in-{server}"));
        look::button(id, look::ButtonKind::Outlined, look::ControlSize::Small, "Sign in")
            .accessibility_label(format!("Sign in to {name}"))
            .off(self.off)
            .on_click(move |_, _, cx| {
                let _ = pane.update(cx, |pane, cx| pane.sign_in_to(&to, cx));
            })
    }

    /// The button that has the agent connect again to the server called `name`, in its row, `row`.
    fn reload(&self, reload: &Reload, name: &str, row: &SharedString) -> look::Button {
        let (pane, server) = (self.pane.clone(), reload.server.clone());
        let id = SharedString::from(format!("mcp-reload-{server}"));
        let button = match reload.offered {
            Offered::Plainly => look::button(id, look::ButtonKind::Outlined, look::ControlSize::Small, "Reload"),
            Offered::Quietly => {
                look::icon_button(id, look::ButtonKind::Quiet, look::ControlSize::Small, IconName::RotateCw)
                    .tooltip("Reload")
                    .shown_while_over(row.clone())
            }
        };
        button.accessibility_label(format!("Reload {name}")).off(self.off).on_click(move |_, _, cx| {
            let _ = pane.update(cx, |pane, cx| pane.reload_tool_server(server.clone(), cx));
        })
    }
}

/// What's over the servers the CLI has of its own: whose they are, and the machine they're set up on.
struct Heading {
    own: SharedString,
    machine: SharedString,
}

fn card(servers: &ChatToolServers, own: Heading, offers: &Offers, cx: &App) -> AnyElement {
    let hubs = section("MCP servers".into(), "Roundhouse".into(), "mcp-hubs", &servers.hubs, offers, cx);
    let listed = |own: Heading, servers| section(own.own, own.machine, "mcp-own", servers, offers, cx);
    let own = match &servers.own {
        OwnToolServers::Listed(servers) if !servers.is_empty() => Some(listed(own, servers)),
        OwnToolServers::NotKnown => Some(section(own.own, "Not known yet".into(), "mcp-own", &[], offers, cx)),
        OwnToolServers::Listed(_) | OwnToolServers::NeverSaid => None,
    };
    let listed = v_flex().id("mcp-listed").max_h(MOST_HEIGHT).overflow_y_scroll().gap(look::Space::Medium);
    let listed = match own {
        // With none of the hub's, what's left needs no rule over it.
        Some(own) if servers.hubs.is_empty() => listed.child(own),
        // In what scrolls, so it goes by with what it's between.
        own => listed.child(hubs).children(own.map(|own| own.line_over_on(CardKind::Over, cx).pt(look::Space::Medium))),
    };
    surface(Region::ToolServersCard).child(listed.test_support()).into_any_element()
}

/// Some of the servers, in rows with IDs that start `id`, under whose they are and where they're kept.
fn section(
    whose: SharedString,
    kept: SharedString,
    id: &'static str,
    servers: &[ChatToolServer],
    offers: &Offers,
    cx: &App,
) -> Div {
    let kept = div().flex_none().typeset(look::Type::Detail).text_color(look::colors(cx).text_faint).child(kept);
    let heading = h_flex()
        .justify_between()
        .gap(look::Space::Medium)
        .child(div().min_w_0().truncate().emphasis().child(whose))
        .child(kept);
    let rows = servers.iter().enumerate().map(|(at, server)| row(format!("{id}-{at}").into(), server, offers, cx));
    v_flex().gap(look::Space::Small).child(heading).children(rows)
}

/// A server, with how it stands at its end, or the button that signs in to it or that has the agent connect to it again.
/// Under it is why it can't be reached or signed in to, or what it's to be signed in to again for, after how it stands
/// where a Reload is in that's place. The name is a person's, so it's the one cut off if they don't both fit, and how it
/// stands always shows.
fn row(id: SharedString, server: &ChatToolServer, offers: &Offers, cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let color = match server.stands.tone() {
        Tone::Plain => colors.text_faint,
        Tone::Coming => colors.text_muted,
        Tone::Problem => colors.warning,
    };
    let words = server
        .stands
        .words()
        .map(|words| div().flex_none().typeset(look::Type::Detail).text_color(color).child(words).into_any_element());
    let sign_in = server.stands.to_sign_in_to().map(|to| offers.sign_in(to, &server.name));
    let reload = server.reload.as_ref().map(|reload| (reload.offered, offers.reload(reload, &server.name, &id)));
    let (at_the_end, words_under) = match (sign_in, reload) {
        (Some(sign_in), _) => (vec![sign_in.into_any_element()], None),
        (None, Some((Offered::Plainly, reload))) => (vec![reload.into_any_element()], words),
        (None, Some((Offered::Quietly, reload))) => {
            (std::iter::once(reload.into_any_element()).chain(words).collect(), None)
        }
        (None, None) => (words.into_iter().collect(), None),
    };
    let line = h_flex()
        .justify_between()
        .items_center()
        .min_h(look::ControlSize::Small.height())
        .gap(look::Space::Medium)
        .child(div().min_w_0().truncate().text_color(colors.text_muted).child(server.name.clone()))
        .child(h_flex().flex_none().items_center().gap(look::Space::Small).children(at_the_end));
    // Read out whole, since a screen reader would read its parts apart, and one to sign in to says so only here.
    v_flex()
        .id(id.clone())
        .role(Role::Label)
        .aria_label(server.said())
        .group(id)
        .child(line)
        .children(under(words_under, server.stands.why(), cx))
        .test_support()
}

/// The line under a server: how it stands, where that isn't at its end, and why, cut off if it doesn't fit.
fn under(stands: Option<AnyElement>, why: Option<&str>, cx: &App) -> Option<impl IntoElement> {
    if stands.is_none() && why.is_none() {
        return None;
    }
    let why = why.map(|why| if stands.is_some() { format!(". {why}") } else { why.to_string() });
    let why = why.map(|why| div().min_w_0().truncate().child(why));
    let line = h_flex().id("mcp-why").typeset(look::Type::Detail).text_color(look::colors(cx).text_faint);
    Some(line.children(stands).children(why).test_support())
}
