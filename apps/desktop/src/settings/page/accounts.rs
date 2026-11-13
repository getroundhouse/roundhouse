//! Accounts, the page of the logins agents run under, in two lists. Found has each agent's own login on a machine,
//! which the hub learnt of as an agent launched under it, and which Roundhouse only reads. Added has the logins the
//! person gave the hub, each a token the agent's own CLI made, which the hub keeps and hands to a machine with none of
//! its own. The app never works out which login an agent will run under, since that's the hub's to decide.

use client_core::Agents;
use client_core::records::Snapshot;
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, Entity, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, ScrollHandle,
    Styled as _, TestSupportExt as _, Window, div,
};
use look::{Off as _, Typeset as _};
use protocol::model::{Account, AgentDescription, AgentKind};

use self::kept::{Kept, To};
use super::{Asking, Body, OfHub, Page, PageId, Within};

mod kept;
mod panel;
mod row;
#[cfg(test)]
mod tests;
mod words;

pub(super) fn page(hub: &OfHub) -> Page {
    let (records, asking) = (hub.records.clone(), hub.asking.clone());
    let draw = Box::new(move |within: &Within| {
        AccountsPage { records: records.clone(), asking: asking.clone(), within: within.clone() }.into_any_element()
    });
    let body = Body::Own { about: words::ABOUT, draw };
    Page { id: PageId::Accounts, name: "Accounts", icon: Lucide::KeyRound, body }
}

/// The page, drawn from the hub's records, with the hub to ask while there's one, within Settings.
#[derive(IntoElement)]
struct AccountsPage {
    records: Snapshot,
    asking: Option<Asking>,
    within: Within,
}

/// What every part of the page is drawn with.
struct Drawing<'a> {
    kept: &'a Entity<Kept>,
    agents: &'a Agents,
    /// The scroll of Settings' page, to bring a panel into sight as it opens.
    scroll: &'a ScrollHandle,
    /// The one moment every row says a time as of.
    now_ms: i64,
}

impl RenderOnce for AccountsPage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let kept = window.use_keyed_state("settings-accounts", cx, |_, _| Kept::default());
        let now_ms = look::now_ms(cx);
        let (found, added) = listed(self.records.accounts());
        let changes_at_ms =
            row::changes_at_ms(found.iter().chain(&added).copied(), now_ms, look::next_day_at_ms(now_ms, cx));
        let focus = self.within.focus.clone();
        kept.update(cx, |kept, cx| kept.drawn(self.asking, focus, changes_at_ms, cx));
        let drawing = Drawing { kept: &kept, agents: self.records.agents(), scroll: &self.within.scroll, now_ms };
        v_flex().gap_8().pb_4().child(found_list(&drawing, &found, cx)).child(added_list(&drawing, &added, cx))
    }
}

/// The accounts in each list, Found then Added, each by what its row calls it, and in Added, the logins the hub holds
/// before those that were disconnected, since they're the ones agents can run under.
fn listed<'a>(accounts: impl Iterator<Item = &'a Account>) -> (Vec<&'a Account>, Vec<&'a Account>) {
    let (mut added, mut found): (Vec<&Account>, Vec<&Account>) =
        accounts.partition(|account| account.login_agent.is_some());
    found.sort_by_cached_key(|account| (words::called(account).to_lowercase(), account.id.clone()));
    added.sort_by_cached_key(|account| {
        (account.held_login.is_none(), words::called(account).to_lowercase(), account.id.clone())
    });
    (found, added)
}

/// Whether the hub is from before hubs kept logins: one that describes its agents, none of them with a login, or that
/// refused to connect one as too old to, as `refused_as_older` says. A hub that hasn't described any yet isn't taken
/// for one, since nothing it said says so.
fn older_hub(agents: &Agents, refused_as_older: bool) -> bool {
    refused_as_older || (agents.named().next().is_some() && agents.with_logins().next().is_none())
}

fn found_list(drawing: &Drawing<'_>, found: &[&Account], cx: &App) -> Div {
    let rows = found.iter().map(|account| row::row(drawing, account, cx));
    let none = found.is_empty().then(|| said_in_the_list("accounts-none-found", words::none_found(drawing.agents), cx));
    let list = list(cx).children(none).children(rows);
    v_flex().child(heading("Found", words::found_says(drawing.agents), None, cx)).child(list)
}

/// Added, with Connect account by its heading. The panel for a new account is under the heading, where the account
/// will be, and the one for an account's own login is at its row. Nothing is said of the hub's age without a hub to
/// ask, and Connect account is off then, as it is with nothing to connect.
fn added_list(drawing: &Drawing<'_>, added: &[&Account], cx: &App) -> Div {
    let kinds: Vec<&AgentDescription> = drawing.agents.with_logins().collect();
    let kept = drawing.kept.read(cx);
    let can_ask = kept.can_ask();
    let older = can_ask && older_hub(drawing.agents, kept.refused_as_older);
    let connect = connect_button(drawing, &kinds, can_ask && !older);
    let panel = kept.panel.as_ref().filter(|panel| matches!(panel.to, To::New { .. }));
    let panel = panel.map(|panel| div().py_4().child(panel::panel(drawing, panel, cx)));
    let said = if older {
        Some(("accounts-older-hub", words::OLDER_HUB))
    } else {
        (added.is_empty() && panel.is_none()).then_some(("accounts-none-added", words::NONE_ADDED))
    };
    let said = said.map(|(id, said)| said_in_the_list(id, said.to_string(), cx));
    let rows = added.iter().map(|account| row::row(drawing, account, cx));
    let list = list(cx).children(panel).children(said).children(rows);
    v_flex().child(heading("Added", words::added_says(), Some(connect), cx)).child(list)
}

/// Connect account, which opens the panel for a new account of the one kind the hub can hold a login for, or a menu
/// of the kinds when it can hold one for several. It's on only while `on` and there's a kind.
fn connect_button(drawing: &Drawing<'_>, kinds: &[&AgentDescription], on: bool) -> AnyElement {
    let button = Button::new("accounts-connect")
        .outline()
        .small()
        .icon(IconName::Plus)
        .label("Connect account")
        .off_while(!on || kinds.is_empty());
    let open = |kept: &Entity<Kept>, agent: AgentKind| {
        let kept = kept.clone();
        move |window: &mut Window, cx: &mut App| kept.update(cx, |kept, cx| kept.open_new(agent, window, cx))
    };
    if let [only] = kinds {
        let open = open(drawing.kept, only.kind);
        return button.on_click(move |_, window, cx| open(window, cx)).into_any_element();
    }
    let choices: Vec<(String, AgentKind)> = kinds.iter().map(|agent| (agent.name.clone(), agent.kind)).collect();
    let kept = drawing.kept.clone();
    button
        .dropdown_menu(move |menu: PopupMenu, _, _| {
            choices.iter().fold(menu, |menu, (name, agent)| {
                let open = open(&kept, *agent);
                menu.item(PopupMenuItem::new(name.clone()).on_click(move |_, window, cx| open(window, cx)))
            })
        })
        .into_any_element()
}

/// A list's name, what it says of itself under that, and what it does at the far end, if anything.
fn heading(name: &'static str, says: String, action: Option<AnyElement>, cx: &App) -> Div {
    let said = v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(div().emphasis().child(name))
        .child(div().typeset(look::Type::Body).text_color(look::colors(cx).text_muted).child(says));
    h_flex().items_end().justify_between().gap_6().pb_3().child(said).children(action)
}

/// The rows of a list, under a line, each with a line under it.
fn list(cx: &App) -> Div {
    v_flex().border_t_1().border_color(look::colors(cx).edge)
}

/// What a list says in place of rows, or before them, known to tests by `id`.
fn said_in_the_list(id: &'static str, said: String, cx: &App) -> impl IntoElement + use<> {
    let colors = look::colors(cx);
    div()
        .id(id)
        .py_4()
        .border_b_1()
        .border_color(colors.edge)
        .typeset(look::Type::Body)
        .text_color(colors.text_muted)
        .child(said)
        .test_support()
}
