//! One hub-wide default for each kind whose adapter can accept a held login.

use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, div, px};
use look::{Off as _, Sizing as _, Typeset as _};
use protocol::model::{Account, AgentDescription};

use super::{Drawing, words};

/// What Default accounts says above its rows, on a hub that keeps defaults.
const INTRO: &str =
    "Choose which account new chats use across this hub. A chat stays on its account when you choose another.";

pub(super) fn section(drawing: &Drawing<'_>, accounts: &[&Account], cx: &App) -> Div {
    let colors = look::colors(cx);
    // A hub that keeps no defaults puts no chat on an account.
    let intro = if drawing.defaults.is_some() { INTRO } else { "Choose which account new chats use across this hub." };
    let intro = div().typeset(look::Type::Body).text_color(colors.text_muted).child(intro);
    let rows = drawing.agents.with_logins().map(|agent| row(drawing, agent, accounts, cx));
    let error = drawing
        .kept
        .read(cx)
        .choosing
        .error
        .as_ref()
        .map(|said| div().typeset(look::Type::Body).text_color(colors.error).child(said.clone()));
    v_flex().gap_3().child(div().emphasis().child("Default accounts")).child(intro).children(rows).children(error)
}

fn row(drawing: &Drawing<'_>, agent: &AgentDescription, accounts: &[&Account], cx: &App) -> Div {
    let selected = drawing.defaults.and_then(|defaults| defaults.iter().find(|choice| choice.agent == agent.kind));
    let account = selected.and_then(|choice| accounts.iter().find(|account| account.id == choice.account).copied());
    let label = match (drawing.defaults, selected, account) {
        (None, _, _) => "Unavailable on this hub".to_string(),
        (_, None, _) => "Machine login".to_string(),
        (_, Some(_), None) => "Loading account…".to_string(),
        (_, _, Some(account)) => words::called(account),
    };
    let blocked = account.is_some_and(|account| !usable(account, drawing.now_ms));
    let says = match (drawing.defaults, selected, account) {
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
    let description = v_flex().flex_1().min_w_0().gap_1().child(agent.name.clone()).children(says);
    h_flex()
        .gap_6()
        .py_3()
        .border_b_1()
        .border_color(colors.edge)
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
        .filter(|account| account.login_agent == Some(agent.kind) && usable(account, drawing.now_ms))
        .map(|account| (account.id.clone(), words::called(account)))
        .collect();
    let kept = drawing.kept.read(cx);
    let disabled =
        drawing.defaults.is_none() || !kept.can_ask() || kept.choosing.pending.is_some() || choices.is_empty();
    let kept = drawing.kept.clone();
    let kind = agent.kind;
    let selected = drawing
        .defaults
        .and_then(|defaults| defaults.iter().find(|choice| choice.agent == kind))
        .map(|choice| choice.account.clone());
    Button::new(SharedString::from(format!("account-default-{}", kind.as_str())))
        .outline()
        .sized(look::ControlSize::Regular)
        .label(label)
        .w(px(200.))
        .icon(IconName::ChevronDown)
        .off_while(disabled)
        .dropdown_menu(move |menu: PopupMenu, _, _| {
            choices.iter().fold(menu, |menu, (account, name)| {
                let (kept, account) = (kept.clone(), account.clone());
                menu.item(PopupMenuItem::new(name.clone()).checked(selected.as_ref() == Some(&account)).on_click(
                    move |_, _, cx| {
                        kept.update(cx, |kept, cx| kept.choose_default(kind, account.clone(), cx));
                    },
                ))
            })
        })
        .into_any_element()
}

fn usable(account: &Account, now_ms: i64) -> bool {
    account.held_login.as_ref().is_some_and(|held| held.ends_at_ms.is_none_or(|ends| ends > now_ms))
}
