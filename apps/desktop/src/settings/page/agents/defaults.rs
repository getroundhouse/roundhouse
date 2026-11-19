//! Default accounts, a list with the hub's one default for each kind whose adapter can accept a held login, and what
//! chooses it, under a heading as the account pages' lists are.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Div, IntoElement, ParentElement as _, Pixels, SharedString, Styled as _, Window, div, px,
};
use look::Lined as _;
use look::Typeset as _;
use protocol::model::{Account, AgentDescription};

use super::super::lists::{heading, list};
use super::Drawing;
use super::choosing::Choice;

/// What a kind's row says while Automatic chooses for it.
const AUTOMATIC_SAYS: &str = "Each new chat goes to the account with the most of its week to spare, short of what's \
                              reserved. Chats already on an account stay there.";

/// How wide the picker's menu is, for an account's name and how much of its week is left beside it.
const MENU_WIDTH: Pixels = px(240.);

/// What Default accounts says above its rows, on a hub that keeps defaults.
const INTRO: &str =
    "Choose which account new chats use across this hub. A chat stays on its account until you move it.";

pub(super) fn section(drawing: &Drawing<'_>, accounts: &[&Account], cx: &App) -> Div {
    let colors = look::colors(cx);
    // A hub that keeps no defaults puts no chat on an account.
    let intro = if drawing.defaults.is_some() { INTRO } else { "Choose which account new chats use across this hub." };
    let rows: Vec<Div> = drawing.agents.with_logins().map(|agent| row(drawing, agent, accounts, cx)).collect();
    // With no kind to choose for, as on a hub from before hubs kept logins, there's no line for rows to go under.
    let rows = (!rows.is_empty()).then(|| list(cx).children(rows));
    let error = drawing.kept.read(cx).choosing.error.as_ref().map(|said| {
        div().pt(look::Space::Medium).typeset(look::Type::Body).text_color(colors.error).child(said.clone())
    });
    v_flex().child(heading("Default accounts", None, intro, None, cx)).children(rows).children(error)
}

fn row(drawing: &Drawing<'_>, agent: &AgentDescription, accounts: &[&Account], cx: &App) -> Div {
    let selected = drawing.defaults.and_then(|defaults| defaults.iter().find(|choice| choice.agent == agent.kind));
    let account = selected.and_then(|choice| accounts.iter().find(|account| account.id == choice.account).copied());
    let automatic = selected.is_some_and(|choice| choice.automatic);
    let label = match (drawing.defaults, selected, account) {
        (None, _, _) => "Unavailable on this hub".to_string(),
        _ if automatic => "Automatic".to_string(),
        (_, None, _) => "Machine login".to_string(),
        (_, Some(_), None) => "Loading account…".to_string(),
        (_, _, Some(account)) => crate::names::called(account),
    };
    // Automatic leaves out an account whose login can't be used, so that one blocks nothing.
    let blocked = !automatic && account.is_some_and(|account| !account.works_for(agent.kind, drawing.now_ms));
    let says = match (drawing.defaults, selected, account) {
        _ if automatic => Some(AUTOMATIC_SAYS),
        _ if blocked => Some(
            "Renew or reconnect it. Until then, chats on it can't start, nor can new chats unless you choose another.",
        ),
        (Some(_), None, _) => Some("Used only while no default account has been added."),
        (Some(_), Some(_), Some(_)) => Some("Chats already on another account stay there."),
        // Nothing to say of a hub that keeps no defaults, or of one whose account is still loading.
        _ => None,
    };
    let colors = look::colors(cx);
    let says = says.map(|says| {
        div().typeset(look::Type::Body).text_color(if blocked { colors.error } else { colors.text_muted }).child(says)
    });
    let description = v_flex().flex_1().min_w_0().gap(look::Space::XSmall).child(agent.name.clone()).children(says);
    h_flex()
        .gap(look::Space::XLarge)
        .py(look::Space::Medium)
        .line_under(cx)
        .child(description)
        .child(picker(drawing, agent, accounts, label, cx))
}

fn picker(
    drawing: &Drawing<'_>,
    agent: &AgentDescription,
    accounts: &[&Account],
    label: String,
    cx: &App,
) -> AnyElement {
    let choices: Vec<_> = accounts
        .iter()
        .filter(|account| account.works_for(agent.kind, drawing.now_ms))
        .map(|account| {
            let week = crate::limits::week_left(account, drawing.now_ms);
            (account.id.clone(), crate::names::called(account), week)
        })
        .collect();
    let kept = drawing.kept.read(cx);
    let disabled =
        drawing.defaults.is_none() || !kept.can_ask() || kept.choosing.pending.is_some() || choices.is_empty();
    let kept = drawing.kept.clone();
    let kind = agent.kind;
    let selected = drawing
        .defaults
        .and_then(|defaults| defaults.iter().find(|choice| choice.agent == kind))
        .map(|choice| if choice.automatic { Choice::Automatic } else { Choice::Account(choice.account.clone()) });
    look::button(
        SharedString::from(format!("account-default-{}", kind.as_str())),
        look::ButtonKind::Outlined,
        look::ControlSize::Regular,
        label,
    )
    .width(px(200.))
    .off(disabled)
    .menu(Anchor::TopRight, move |menu: look::Menu, _, _| {
        let choose = |choice: Choice| {
            let kept = kept.clone();
            move |_: &mut Window, cx: &mut App| {
                kept.update(cx, |kept, cx| kept.choose_default(kind, choice.clone(), cx));
            }
        };
        let automatic = look::menu_row("Automatic").note("Most to spare", look::NoteKind::Detail);
        let automatic = automatic.chosen(selected == Some(Choice::Automatic)).on_choose(choose(Choice::Automatic));
        let menu = menu.width(MENU_WIDTH).row(automatic).separator();
        choices.iter().fold(menu, |menu, (account, name, week)| {
            let choice = Choice::Account(account.clone());
            let row = look::menu_row(name.clone()).chosen(selected.as_ref() == Some(&choice));
            let row = match week {
                Some(week) => row.note(week.clone(), look::NoteKind::Detail),
                None => row,
            };
            menu.row(row.on_choose(choose(choice)))
        })
    })
    .into_any_element()
}
