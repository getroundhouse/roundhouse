//! MCP servers as Settings lists it, and the page as it's drawn: Added, in a column, each frame from the hub's records as
//! client-core holds them, with what the page keeps from one frame to the next.

use client_core::records::Snapshot;
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, Context, Entity, IntoElement, ParentElement as _, RenderOnce, ScrollHandle, Styled as _, Window, px,
};

use super::super::account_row::{COLUMN_WIDTH, Columns};
use super::kept::Kept;
use super::{Asking, Body, OfHub, Page, PageId, Within, added, words};
use crate::tool_sign_in::ToolSigningIn;

/// How wide a row's middle columns are: the server's address, and what else there is to say, such as `Key`.
pub(super) const COLUMNS: Columns = Columns { how: px(190.), detail: px(190.) };

pub(in super::super) fn page(hub: &OfHub) -> Page {
    let (records, asking, signing_in) = (hub.records.clone(), hub.asking.clone(), hub.tool_sign_in.clone());
    let draw = Box::new(move |within: &Within| {
        let (records, asking, signing_in) = (records.clone(), asking.clone(), signing_in.clone());
        ServersPage { records, asking, signing_in, within: within.clone() }.into_any_element()
    });
    let body = Body::Own { about: words::ABOUT, draw };
    Page { id: PageId::ToolServers, name: "MCP servers", icon: Lucide::Plug, body }
}

/// The page, drawn from the hub's records, with the hub to ask while there's one, within Settings.
#[derive(IntoElement)]
struct ServersPage {
    records: Snapshot,
    asking: Option<Asking>,
    signing_in: Entity<ToolSigningIn>,
    within: Within,
}

/// What every part of the page is drawn with.
pub(super) struct Drawing<'a> {
    pub(super) kept: &'a Entity<Kept>,
    pub(super) records: &'a Snapshot,
    /// The scroll of Settings' page, to bring a panel into sight as it opens.
    pub(super) scroll: &'a ScrollHandle,
}

impl Drawing<'_> {
    /// Whether the hub's records have arrived, so one that lists no server has none.
    pub(super) fn arrived(&self) -> bool {
        self.records.hub_settings().is_some()
    }

    /// Whether the page can ask the hub to change something now: there's a hub, and its records have arrived.
    pub(super) fn can_ask(&self, cx: &App) -> bool {
        self.arrived() && self.kept.read(cx).can_ask()
    }

    /// What a panel's button does to the page's state.
    pub(super) fn act(
        &self,
        act: fn(&mut Kept, &mut Window, &mut Context<Kept>),
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let kept = self.kept.clone();
        move |window, cx| kept.update(cx, |kept, cx| act(kept, window, cx))
    }
}

impl RenderOnce for ServersPage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // By the hub's sign-in, so it's never the state of a page of another hub's.
        let (of_hub, signing_in) = (("settings-mcp", self.signing_in.entity_id()), self.signing_in);
        let kept = window.use_keyed_state(of_hub, cx, |window, cx| Kept::new(signing_in, window, cx));
        let focus = self.within.focus.clone();
        kept.update(cx, |kept, _| kept.drawn(self.asking, focus));
        let drawing = Drawing { kept: &kept, records: &self.records, scroll: &self.within.scroll };
        v_flex().w_full().max_w(COLUMN_WIDTH).pb(look::Space::Large).child(added::list(&drawing, cx))
    }
}
