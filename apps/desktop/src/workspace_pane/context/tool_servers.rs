//! The plug beside Send, and its card, which lists the MCP servers the chat has: the hub's, then the ones its agent's CLI
//! has of its own. What's said of each is `client_core::tool_servers`'s, and signing in to one is the hub's view's, in
//! `crate::tool_sign_in`, so every chat's card says the same of it.

use client_core::tool_servers::{ChatToolServer, ChatToolServers, OwnToolServers, Tone};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, div, px,
};
use look::{CardKind, IconSizing as _, Lined as _, Region, Typeset as _};
use protocol::model::ToolServerId;

use super::super::WorkspacePane;
use super::super::composer::{Hub, Slot, middle};
use super::card::surface;
use super::usage::card_over;
use crate::tool_sign_in::{SignInTo, UnderWay};

/// How tall the card's servers get before they scroll, so a long list stays in the window: about ten rows.
const MOST_HEIGHT: Pixels = px(360.);

impl WorkspacePane {
    /// The plug for the chat showing, while there's something to say of its servers. It's as quiet as the ring, or amber
    /// while one of the hub's needs the person to sign in to it.
    pub(in super::super) fn plug(&self, cx: &mut Context<Self>) -> Option<Slot> {
        let sign_in = self.tool_sign_in.read(cx).under_way().and_then(UnderWay::of_a_server);
        let servers = ChatToolServers::of(self.shown.all(), self.session()?, sign_in.as_ref())?;
        let colors = look::colors(cx);
        let color = if servers.needs_sign_in() { colors.warning } else { colors.text_muted };
        let icon = Icon::new(IconName::Plug).sized(look::IconSize::Large).text_color(color);
        // The whole slot is the plug's to point at, as it is the ring's.
        let plug = middle().size(Slot::SIZE.height()).child(icon);
        // Said in words too, since a colour alone says nothing to someone who can't see it.
        let said = servers.said();
        let heading = Heading {
            own: match self.agent() {
                Some(agent) => format!("{}'s own", agent.name).into(),
                None => "The agent's own".into(),
            },
            machine: crate::names::runs_on(self.workspace()),
        };
        let sign_in = OffersSignIn { pane: cx.weak_entity(), off: self.hub == Hub::Away };
        // It holds a button while one needs signing in to, so it waits for the pointer to cross to it.
        let kind = if servers.needs_sign_in() { look::hover_card_with_a_button } else { look::hover_card };
        let hover = card_over(kind("mcp-hover"), plug).content(move |_, _, cx| card(&servers, heading, &sign_in, cx));
        Some(Slot::holding(div().id("mcp").role(Role::Status).aria_label(said).child(hover).test_support()))
    }

    /// Signs in to the hub's `server` again, for every chat, if the hub has it still.
    fn sign_in_to(&mut self, server: &ToolServerId, cx: &mut Context<Self>) {
        if let Some(server) = self.shown.all().tool_server(server) {
            let again = SignInTo::again(server);
            self.tool_sign_in.update(cx, |signing_in, cx| signing_in.start(again, cx));
        }
    }
}

/// What a row's Sign in button does it through.
struct OffersSignIn {
    pane: WeakEntity<WorkspacePane>,
    /// With no hub to ask.
    off: bool,
}

impl OffersSignIn {
    /// The button that signs in to `server`, which the person calls `name`.
    fn button(&self, server: &ToolServerId, name: &str) -> look::Button {
        let (pane, to) = (self.pane.clone(), server.clone());
        let id = SharedString::from(format!("mcp-sign-in-{server}"));
        look::button(id, look::ButtonKind::Outlined, look::ControlSize::Small, "Sign in")
            .accessibility_label(format!("Sign in to {name}"))
            .off(self.off)
            .on_click(move |_, _, cx| {
                let _ = pane.update(cx, |pane, cx| pane.sign_in_to(&to, cx));
            })
    }
}

/// What's over the servers the CLI has of its own: whose they are, and the machine they're set up on.
struct Heading {
    own: SharedString,
    machine: SharedString,
}

fn card(servers: &ChatToolServers, own: Heading, sign_in: &OffersSignIn, cx: &App) -> AnyElement {
    let hubs = section("MCP servers".into(), "Roundhouse".into(), "mcp-hubs", &servers.hubs, sign_in, cx);
    let listed = |own: Heading, servers| section(own.own, own.machine, "mcp-own", servers, sign_in, cx);
    let own = match &servers.own {
        OwnToolServers::Listed(servers) if !servers.is_empty() => Some(listed(own, servers)),
        OwnToolServers::NotKnown => Some(section(own.own, "Not known yet".into(), "mcp-own", &[], sign_in, cx)),
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
    sign_in: &OffersSignIn,
    cx: &App,
) -> Div {
    let kept = div().flex_none().typeset(look::Type::Detail).text_color(look::colors(cx).text_faint).child(kept);
    let heading = h_flex()
        .justify_between()
        .gap(look::Space::Medium)
        .child(div().min_w_0().truncate().emphasis().child(whose))
        .child(kept);
    let rows = servers.iter().enumerate().map(|(at, server)| row((id, at), server, sign_in, cx));
    v_flex().gap(look::Space::Small).child(heading).children(rows)
}

/// A server, with how it stands at its end, or the button that signs in to it, and under it why it can't be reached or
/// signed in to. The name is a person's, so it's the one cut off if they don't both fit, and how it stands always shows.
fn row(id: (&'static str, usize), server: &ChatToolServer, sign_in: &OffersSignIn, cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let (words, why) = (server.stands.words(), server.stands.why());
    let color = match server.stands.tone() {
        Tone::Plain => colors.text_faint,
        Tone::Coming => colors.text_muted,
        Tone::Problem => colors.warning,
    };
    let said: Vec<&str> = [Some(server.name.as_str()), words.as_deref(), why].into_iter().flatten().collect();
    // Read out whole, since a screen reader would read its parts apart, and one to sign in to says so only here.
    let row = v_flex().id(id).role(Role::Label).aria_label(said.join(", "));
    let stands = match server.stands.to_sign_in_to() {
        Some(to) => Some(sign_in.button(to, &server.name).into_any_element()),
        None => words.map(|words| {
            div().flex_none().typeset(look::Type::Detail).text_color(color).child(words).into_any_element()
        }),
    };
    let line = h_flex()
        .justify_between()
        .items_center()
        .min_h(look::ControlSize::Small.height())
        .gap(look::Space::Medium)
        .child(div().min_w_0().truncate().text_color(colors.text_muted).child(server.name.clone()))
        .children(stands);
    let why = why.map(|why| {
        div()
            .id("mcp-why")
            .truncate()
            .typeset(look::Type::Detail)
            .text_color(colors.text_faint)
            .child(why.to_string())
            .test_support()
    });
    row.child(line).children(why).test_support()
}
