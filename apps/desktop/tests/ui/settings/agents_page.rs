//! What Agents in Settings draws of the logins a hub holds, and how its page moves, on a fake hub, since none of it
//! asks the hub to change anything: a row warning of its login's end by the page's own clock, a panel opening at its
//! row in sight, and the page stretching as it's pulled. Every account is plainly made up.

#![cfg(test)]

use std::time::Duration;

use client_core::fake_hub::World;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ScrollDelta, TestAppContext, point, px};
use protocol::model::AccountId;

use super::agents::{id, record, says, to_agents};
use super::made_up::{DAY_MS, added, holding_logins, in_agents_of};
use crate::support::{FakeHub, Ui, click_menu_item};

const MINUTE_MS: i64 = 60_000;

/// Held still a minute before a login starts to warn of its end, then a minute after it, with nothing changed that
/// would draw the page again, so only its own clock can.
#[gpui_kit::test]
fn a_login_starts_to_warn_a_month_before_it_ends_with_nothing_else_drawing_the_page_again(cx: &mut TestAppContext) {
    let work = added("claude-login-0001", "Work", 200 * DAY_MS);
    let account = work.id.clone();
    let fake = FakeHub::holding(holding_logins(vec![work]));
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("the account", |ui| record(ui, &account).is_some());
    let warns_at_ms = record(&ui, &account).unwrap().held_login.unwrap().ends_at_ms.unwrap() - 30 * DAY_MS;
    ui.cx.update(|cx| look::hold_still(warns_at_ms - MINUTE_MS, cx));
    to_agents(&mut ui);
    assert_eq!(says(&mut ui, &account), "No limits heard of yet");

    ui.cx.update(|cx| look::hold_still(warns_at_ms + MINUTE_MS, cx));
    ui.cx.executor().advance_clock(Duration::from_mins(2));
    ui.frame();

    assert!(says(&mut ui, &account).contains("Ends in"), "{}", says(&mut ui, &account));
}

/// More logins than fit, in the order Added lists them, which is by name.
fn more_than_fit() -> (World, Vec<AccountId>) {
    let accounts: Vec<_> =
        (1..=16).map(|at| added(&format!("claude-login-{at:04}"), &format!("Account {at:02}"), 200 * DAY_MS)).collect();
    let ids = accounts.iter().map(|account| account.id.clone()).collect();
    (holding_logins(accounts), ids)
}

#[gpui_kit::test]
fn renewing_a_login_opens_its_panel_at_its_row_in_sight(cx: &mut TestAppContext) {
    let (world, accounts) = more_than_fit();
    let (_hub, mut ui) = in_agents_of(cx, world);
    let last = accounts.last().unwrap();
    ui.scroll("settings-body", ScrollDelta::Pixels(point(px(0.), px(-10_000.))));
    assert!(ui.visible(id("account-more", last)), "the last row is at the bottom");
    ui.click(id("account-more", last));

    // The scroll is set as the panel is first laid out, during a draw, so it shows only in a frame drawn after. The
    // page asks for one, since nothing else may draw again. Here closing the menu draws a second time before the next
    // frame runs, and the focused token field reports its caret on both draws, so three callbacks wait on the frame:
    // the page's refresh and those two caret reports.
    ui.cx.update_window(ui.handle, |_, window, cx| click_menu_item(window, "Renew", cx)).unwrap();
    assert_eq!(ui.bring_the_next_frame(), 3, "the page asks for the frame that shows the scroll");

    let shows = ui.cx.update_window(ui.handle, |_, window, _| window.find("accounts-panel-connect").visible());
    assert!(shows.unwrap(), "the whole panel shows");
    let (row, panel) = (ui.bounds(id("account", last)), ui.bounds("accounts-panel"));
    assert!(row.top() < panel.top() && panel.bottom() <= row.bottom(), "under its row: {row:?}, {panel:?}");
}

/// As a Mac's own pages do, which a test's window holds still unless it's let move.
#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn a_page_pulled_past_its_top_stretches_as_a_macs_does(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent as _, ScrollEvent, TouchPhase};

    let (world, accounts) = more_than_fit();
    let first = &accounts[0];
    let (_hub, mut ui) = in_agents_of(cx, world);
    let rests_at = ui.bounds(id("account", first)).top();
    ui.allow_motion();

    let on = ui.bounds("settings-body").center();
    for (phase, by) in [(TouchPhase::Started, 0.), (TouchPhase::Moved, 100.), (TouchPhase::Moved, 100.)] {
        let pulled = ScrollEvent {
            position: on,
            delta: ScrollDelta::Pixels(point(px(0.), px(by))),
            touch_phase: phase,
            ..Default::default()
        };
        ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(pulled.to_platform_input(), cx)).unwrap();
        ui.frame();
    }
    ui.bring_the_next_frame();
    ui.frame();

    let pulled_to = ui.bounds(id("account", first)).top();
    assert!(pulled_to > rests_at, "it stretches down from {rests_at:?}, to {pulled_to:?}");
}
