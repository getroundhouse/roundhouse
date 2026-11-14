//! One hub-wide default for each kind whose adapter can accept a held login.

use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, div, px};
use look::{Off as _, Sizing as _, Typeset as _};
use protocol::model::{Account, AgentDescription};

use super::{Drawing, words};

pub(super) fn section(drawing: &Drawing<'_>, accounts: &[&Account], cx: &App) -> Div {
    let colors = look::colors(cx);
    let intro = div()
        .typeset(look::Type::Body)
        .text_color(colors.text_muted)
        .child("Choose which account each agent uses across this hub. Changes apply when an agent next starts.");
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
    let says = if blocked {
        "Renew or reconnect this account, or choose another. New agents cannot start."
    } else if selected.is_none() && drawing.defaults.is_some() {
        "Used only while no default account has been added."
    } else {
        "Running agents keep their current login."
    };
    let colors = look::colors(cx);
    let description = v_flex().flex_1().min_w_0().gap_1().child(agent.name.clone()).child(
        div().typeset(look::Type::Body).text_color(if blocked { colors.error } else { colors.text_muted }).child(says),
    );
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
