//! The card an account's row in Agents shows, against a real hub: once the pointer has rested on the row, or at once
//! as the keyboard's focus comes to it, saying what the row's one line has no room for, which assistive apps are told
//! too. Every token is plainly made up.

#![cfg(test)]

use gpui_kit::test::{ElementSnapshot, TestQueryExt as _};
use gpui_kit::{AppContext as _, ElementId, Role, TestAppContext};
use look::Region;
use protocol::model::AccountId;

use super::agents::{connected, id, in_agents, record, to_agents};
use crate::support::{Stack, Ui};

const TOKEN: &str = "a-made-up-token";
/// More than there are stops for Tab on Agents before the first account's row.
const MOST_TABS: usize = 40;

/// What the card that's open says, line by line, as it tells assistive apps.
fn card_says(ui: &mut Ui<'_>) -> Vec<String> {
    ui.frame();
    let card = ElementId::Name(Region::AccountCard.id().into());
    let labels = ui.cx.update_window(ui.handle, |_, window, _| window.find_all_by_role(Role::Label)).unwrap();
    let in_card = labels.into_iter().filter(|found| found.path().contains(&card));
    in_card.filter_map(|found| found.label().map(str::to_string)).collect()
}

/// The line of `account`'s row, as assistive apps are told it: what it is, with what its card says.
fn line(ui: &mut Ui<'_>, account: &AccountId) -> ElementSnapshot {
    ui.frame();
    let row = ElementId::Name(id("account", account));
    let groups = ui.cx.update_window(ui.handle, |_, window, _| window.find_all_by_role(Role::Group)).unwrap();
    groups.into_iter().find(|found| found.path().contains(&row)).expect("the row's line")
}

#[gpui_kit::test]
fn resting_on_an_agent_accounts_row_shows_when_it_was_last_used_when_its_login_ends_and_whats_left_of_its_limits(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", TOKEN);
    // A turn under Work, the default, so the hub records its use and its agent hears its limits.
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));
    ui.wait("its use and its limits", |ui| {
        record(ui, &work).is_some_and(|a| a.used_at_ms.is_some() && !a.limits.is_empty())
    });
    let account = record(&ui, &work).unwrap();
    // Held still where it was used, so it was used today, whenever this runs.
    ui.cx.update(|cx| look::hold_still(account.used_at_ms.unwrap(), cx));
    let ends = ui.cx.update(|cx| look::date(account.held_login.unwrap().ends_at_ms.unwrap(), cx)).unwrap();
    to_agents(&mut ui);

    ui.hover(id("account", &work));
    assert!(!ui.has(Region::AccountCard.id()), "passing over the row shows nothing");
    ui.wait_out_a_tooltip();
    assert!(ui.visible(Region::AccountCard.id()), "resting on it does");
    let (card, row) = (ui.bounds(Region::AccountCard.id()), ui.bounds(id("account", &work)));
    assert!(!card.intersects(&row), "the card, {card:?}, covers its row, {row:?}");
    let said = card_says(&mut ui);

    assert_eq!(said[..2], ["Last used today".to_string(), format!("Ends {ends}")], "though its row says neither");
    assert!(said.len() > 2, "with its limits: {said:?}");
    assert!(said[2..].iter().all(|limit| limit.contains(" limit, ") && limit.contains("% left")), "{said:?}");
    let told = line(&mut ui, &work);
    let sentences = said.iter().map(|line| format!("{line}.")).collect::<Vec<_>>().join(" ");
    assert_eq!(told.description(), Some(sentences.as_str()), "assistive apps are told what it says");
    ui.hover("settings-page-agents");
    ui.wait("the card to go", |ui| !ui.has(Region::AccountCard.id()));
}

#[gpui_kit::test]
fn the_keyboards_focus_on_an_agent_accounts_row_shows_its_card_at_once_until_it_moves_on(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", TOKEN);
    let mut ui = in_agents(cx, &stack);
    let ends = {
        let ends_at_ms = record(&ui, &work).unwrap().held_login.unwrap().ends_at_ms.unwrap();
        ui.cx.update(|cx| look::date(ends_at_ms, cx)).unwrap()
    };

    // Without the clock moving on, so with no wait.
    let tabs = (1..=MOST_TABS).find(|_| {
        ui.press("tab");
        ui.has(Region::AccountCard.id())
    });
    assert!(tabs.is_some(), "no stop of Tab's showed the card");
    let told = line(&mut ui, &work);
    let said = card_says(&mut ui);
    ui.press("tab");

    assert_eq!(told.focused(), Some(true), "its line has the focus");
    assert_eq!(told.label(), Some("Work, Default, Claude Code"), "what it is, as the line says");
    assert_eq!(said, [format!("Ends {ends}"), "No limits heard of yet".to_string()], "nobody has used it yet");
    assert_eq!(told.description(), Some(format!("Ends {ends}. No limits heard of yet.").as_str()));
    ui.wait("the card to go, as Tab moves on to the row's menu", |ui| !ui.has(Region::AccountCard.id()));
}
