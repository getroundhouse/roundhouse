//! Moving a chat to another account the hub holds a login for, from the menu beside whose account it is, against a real
//! hub with two added accounts. Every token is plainly made up.

#![cfg(test)]

use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{AccountId, AgentKind, LoginToken, SessionStatus};

use crate::support::{Gate, MENU, Stack, Ui};

/// A login the hub was given from elsewhere, called `name`.
fn connected(stack: &Stack, name: &str, token: &str) -> AccountId {
    stack.client().account_connect(AgentKind::Claude, name, LoginToken::new(token), None).unwrap().id
}

/// A new workspace whose chat has had a turn under Work, which is the default, being added first, so the chat is on it,
/// with Personal added too, so there's somewhere to move it. Its agent heard of Work's limits, so they have bars.
fn under_work<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, AccountId, AccountId) {
    let work = connected(stack, "Work", "work-token");
    let personal = connected(stack, "Personal", "personal-token");
    let mut ui = Ui::open_with_a_repository(cx, stack);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("enter");
    ui.wait_idle_with("the turn to finish", |text| text.contains("hello"));
    ui.wait("the limits' bars", |ui| ui.has("usage"));
    (ui, work, personal)
}

/// The account the chat showing is on, as the app shows it.
fn on(ui: &Ui<'_>) -> Option<AccountId> {
    let pane = ui.pane()?;
    ui.cx.read_entity(&pane, |pane, _| pane.session().and_then(|session| session.hub_account().cloned()))
}

/// The account the chat showing names, as the app shows it.
fn named(ui: &Ui<'_>) -> Option<AccountId> {
    let pane = ui.pane()?;
    ui.cx.read_entity(&pane, |pane, _| pane.session().and_then(|session| session.account.clone()))
}

/// Rests on `slot`, the bars or the ring, whose card says whose account it is, and opens the menu beside that.
fn open_the_menu(ui: &mut Ui<'_>, slot: &str) {
    ui.hover(slot.to_string());
    ui.wait_out_a_tooltip();
    ui.click("account-move");
    ui.wait("the menu", |ui| ui.visible(MENU));
}

#[gpui_kit::test]
fn moving_a_chat_whose_agent_is_working_asks_first_and_moves_it_only_if_told_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, work, personal) = under_work(cx, &stack);
    let done = Gate::new(&stack, "done");
    ui.paste_text(&format!("hang-until:{done}"));
    ui.press("enter");
    ui.wait("the agent to start working", |ui| ui.session_status() == Some(SessionStatus::Running));

    open_the_menu(&mut ui, "usage");
    ui.click_menu_item("Personal");
    ui.wait("the question", |ui| ui.asked().is_some());
    ui.click("move-chat-keep");
    ui.frame();
    open_the_menu(&mut ui, "usage");
    ui.click_menu_item("Personal");
    ui.wait("the question again", |ui| ui.asked().is_some());
    // The hub refused this move as needing force, so the first answer moved nothing.
    let kept = (on(&ui), ui.session_status());
    ui.click("move-chat-act");
    ui.wait("the chat to move", |ui| on(ui).as_ref() == Some(&personal));
    ui.wait("its agent to stop", |ui| ui.session_status() != Some(SessionStatus::Running));

    assert_eq!(kept, (Some(work), Some(SessionStatus::Running)), "kept working, and not moved");
}

/// The chat moves to Personal from the menu beside the bars, and back from the one beside the ring. Personal is
/// disconnected meanwhile, which the chat says until it's moved back, and once it's back on Work, there's no other
/// login that works, so nowhere to move it.
#[gpui_kit::test]
fn the_menu_beside_whose_account_it_is_moves_the_chat_and_back_and_a_lost_login_says_so_until_then(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, work, personal) = under_work(cx, &stack);
    let before = on(&ui);
    open_the_menu(&mut ui, "usage");
    // The menu is drawn outside the card, so with the pointer on it, only the menu keeps the card open, and with it
    // the menu.
    ui.hover(MENU);
    ui.wait_out_a_tooltip();
    let held = ui.visible("usage-card") && ui.visible(MENU);
    ui.click_menu_item("Personal");
    ui.wait("the chat to move", |ui| on(ui).as_ref() == Some(&personal));
    let moved_names = named(&ui);
    // Nothing has been heard of Personal's limits, so there are no bars, and the ring's card says whose it is.
    ui.wait("the bars to go", |ui| !ui.has("usage"));
    stack.client().account_disconnect(&personal).unwrap();

    ui.input("say:blocked");
    ui.press("enter");
    ui.wait("the agent not to start", |ui| ui.events().iter().any(|e| failed_to_start(&e.body)));
    let said = ui.events().into_iter().find(|e| failed_to_start(&e.body)).map(|e| e.body);
    open_the_menu(&mut ui, "context");
    let menu = ElementId::Name(MENU.into());
    let listed = ui.labelled("Personal").iter().any(|item| item.path().contains(&menu));
    // Work's row says how much of its week is left, as the fake agent heard it.
    ui.click_menu_item("Work, 96% of 7d");
    ui.wait("the chat to move back", |ui| on(ui).as_ref() == Some(&work));
    let hubs = stack.client().records_list().unwrap().sessions.into_iter().next().unwrap();
    let hubs = (hubs.hub_account().cloned(), hubs.account);
    ui.input("say:unblocked");
    ui.press("enter");
    ui.wait_idle_with("a turn under Work", |text| text.contains("unblocked"));
    ui.wait("Work's bars", |ui| ui.has("usage"));
    ui.hover("usage".to_string());
    ui.wait_out_a_tooltip();
    ui.wait("the card", |ui| ui.visible("usage-card"));
    let nowhere_to_go = !ui.has("account-move");

    assert_eq!(before.as_ref(), Some(&work), "its first turn put it on the default");
    assert!(held, "the card and its menu stayed open with the pointer on the menu");
    assert_eq!(moved_names, Some(personal), "the chat names the account it was moved to");
    assert_eq!(hubs, (Some(work.clone()), Some(work)), "moved back, the hub has it as the app shows it");
    assert!(listed, "the menu still says which account the chat is on, though it can't be chosen");
    let Some(EventBody::AgentExited { stderr_tail, .. }) = said else { panic!("{said:?}") };
    assert!(stderr_tail.iter().any(|line| line.contains("Personal needs a login")), "{stderr_tail:?}");
    assert!(nowhere_to_go, "no Move with no other login that works");
}

fn failed_to_start(body: &EventBody) -> bool {
    matches!(body, EventBody::AgentExited { reason: ExitReason::StartFailed, .. })
}

/// A new chat that Automatic finds no account to spare for says so in a card, without starting, and `Start on` puts it
/// on the account, where what's sent again goes.
#[gpui_kit::test]
fn a_chat_with_no_account_to_spare_says_so_until_its_started_on_one(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let work = connected(&stack, "Work", "work-token");
    for account in [&work, &connected(&stack, "Personal", "personal-token")] {
        stack.client().account_set_reserve(account, Some(protocol::model::Reserve::ALL)).unwrap();
    }
    stack.client().account_set_automatic(AgentKind::Claude).unwrap();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("enter");
    let start_on_work = format!("start-on-{work}");

    ui.wait("the card", |ui| ui.has(start_on_work.clone()));
    assert_eq!(on(&ui), None);
    ui.click(start_on_work.clone());
    ui.wait("the chat to be on Work", |ui| on(ui).as_ref() == Some(&work));
    ui.wait("the card to go", |ui| !ui.has(start_on_work.clone()));
    ui.input("say:again");
    ui.press("enter");
    ui.wait_idle_with("a turn under Work", |text| text.contains("again"));
}
