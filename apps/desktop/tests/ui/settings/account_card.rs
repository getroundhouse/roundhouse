//! The card an account's row in Agents shows, on a fake hub, since it asks the hub for nothing: once the pointer has
//! rested on the row, or at once as the keyboard's focus comes to it, saying what the row's one line has no room for,
//! which assistive apps are told too. What a card says of an account is `settings::page::agents`' rule, which its unit
//! tests pin, and that a turn records an account's use and its limits is the backend's, which its tests pin. Every
//! account is plainly made up.

#![cfg(test)]

use client_core::fake_hub::NOW_MS;
use gpui_kit::test::{ElementSnapshot, TestQueryExt as _};
use gpui_kit::{AppContext as _, ElementId, Role, TestAppContext};
use look::Region;
use protocol::fixtures::observed;
use protocol::model::{Account, AccountId, LimitWindow, Observation, UsageLimit};

use super::agents::{id, to_agents};
use super::made_up::{DAY_MS, added, holding_logins};
use crate::support::{FakeHub, Ui};

/// More than there are stops for Tab on Agents before the first account's row.
const MOST_TABS: usize = 40;
const HOUR_MS: i64 = 60 * 60_000;

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

fn limit(window: LimitWindow, used_percent: u8, resets_in_ms: i64) -> UsageLimit {
    UsageLimit { window, model: None, used_percent, resets_at_ms: Some(NOW_MS + resets_in_ms), heard_at_ms: None }
}

/// Work, the default, which nobody has used yet, listed first, then Workstation, used an hour ago.
fn work_and_workstation() -> (Account, Account) {
    let work = added("claude-login-0001", "Work", 200 * DAY_MS);
    let workstation =
        Account { used_at_ms: Some(NOW_MS - HOUR_MS), ..added("claude-login-0002", "Workstation", 300 * DAY_MS) };
    (work, workstation)
}

/// The limits Workstation's agent heard.
fn workstations_limits() -> Observation {
    let limits = vec![limit(LimitWindow::FiveHours, 29, 2 * HOUR_MS), limit(LimitWindow::SevenDays, 4, 3 * DAY_MS)];
    observed::limits("claude-login-0002", limits)
}

/// One walk through one window: the card of a used account by the pointer, then the card of one nobody has used by
/// the keyboard.
#[gpui_kit::test]
fn an_agent_accounts_card_shows_as_the_pointer_rests_on_its_row_or_at_once_as_the_keyboards_focus_comes_to_it(
    cx: &mut TestAppContext,
) {
    let (work, used) = work_and_workstation();
    let date = |ui: &mut Ui<'_>, account: &Account| {
        let ends_at_ms = account.held_login.as_ref().unwrap().ends_at_ms.unwrap();
        ui.cx.update(|cx| look::date(ends_at_ms, cx)).unwrap()
    };
    let mut world = holding_logins(vec![work.clone(), used.clone()]);
    world.observe(workstations_limits());
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    // Held still after it was used that day, so it was used today, whenever this runs.
    ui.cx.update(|cx| look::hold_still(NOW_MS, cx));
    let (work_ends, used_ends) = (date(&mut ui, &work), date(&mut ui, &used));
    to_agents(&mut ui);

    ui.hover(id("account", &used.id));
    assert!(!ui.has(Region::AccountCard.id()), "the pointer: passing over the row shows nothing");
    ui.wait_out_a_tooltip();
    assert!(ui.visible(Region::AccountCard.id()), "the pointer: resting on it does");
    let (card, row) = (ui.bounds(Region::AccountCard.id()), ui.bounds(id("account", &used.id)));
    assert!(!card.intersects(&row), "the pointer: the card, {card:?}, covers its row, {row:?}");
    let said = card_says(&mut ui);
    assert_eq!(said[..2], ["Last used today".to_string(), format!("Ends {used_ends}")], "though its row says neither");
    assert!(said.len() > 2, "the pointer: with its limits: {said:?}");
    assert!(said[2..].iter().all(|limit| limit.contains(" limit, ") && limit.contains("% left")), "{said:?}");
    let told = line(&mut ui, &used.id);
    let sentences = said.iter().map(|line| format!("{line}.")).collect::<Vec<_>>().join(" ");
    assert_eq!(told.description(), Some(sentences.as_str()), "the pointer: assistive apps are told what it says");
    ui.hover("settings-page-agents");
    ui.wait("the pointer's card to go", |ui| !ui.has(Region::AccountCard.id()));

    // Without the clock moving on, so with no wait.
    let tabs = (1..=MOST_TABS).find(|_| {
        ui.press("tab");
        ui.has(Region::AccountCard.id())
    });
    assert!(tabs.is_some(), "the keyboard: no stop of Tab's showed the card");
    let told = line(&mut ui, &work.id);
    let said = card_says(&mut ui);
    ui.press("tab");

    assert_eq!(told.focused(), Some(true), "the keyboard: its line has the focus");
    assert_eq!(told.label(), Some("Work, Default, Claude Code"), "the keyboard: what it is, as the line says");
    assert_eq!(said, [format!("Ends {work_ends}"), "No limits heard of yet".to_string()], "nobody has used it yet");
    assert_eq!(told.description(), Some(format!("Ends {work_ends}. No limits heard of yet.").as_str()));
    ui.wait("the keyboard's card to go, as Tab moves on to the row's menu", |ui| !ui.has(Region::AccountCard.id()));
}
