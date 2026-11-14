//! Moving the chat showing to another account the hub holds a login for, from a menu beside whose account it is, on
//! whichever card says that. The hub decides whether moving needs the person's say, since only it knows whether the
//! chat's agent is at work, so this asks them only once it's refused.

use client_core::ClientError;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::{Anchor, AnyElement, Context, IntoElement, PromptLevel, SharedString, WeakEntity, Window};
use look::{Hand as _, HoldOpen, Off as _, Sizing as _};
use protocol::client::ErrorCode;
use protocol::model::{Account, AccountId, AgentKind, Session, SessionId};

use super::super::composer::Hub;
use super::super::{PaneEvent, WorkspacePane};

/// What the menu offers for the chat showing, worked out each time the pane is drawn.
#[derive(Clone)]
pub(in super::super) struct Moves {
    session: SessionId,
    /// Each account the chat can be moved to, by what it's called: those added for its kind whose login works, and the
    /// one it's on, whatever its login, so the menu says which it's on.
    accounts: Vec<Choice>,
    /// The account the chat is on. `None` until its agent first starts under a login the hub holds.
    on: Option<AccountId>,
    /// Whether there's no hub to ask.
    off: bool,
    pane: WeakEntity<WorkspacePane>,
}

/// An account the menu offers, by what it's called.
#[derive(Clone, Debug, PartialEq)]
struct Choice {
    id: AccountId,
    called: SharedString,
    /// Whether its login works, so the chat could run under it. One that doesn't is offered off.
    works: bool,
}

/// What the menu offers a chat of `agent`'s kind that's `on` an account, of `accounts` at `now_ms`, in order of what
/// they're called: each login added for its kind that works, and the one it's on, whatever its login, so the menu says
/// which it's on. `None` when there's nowhere else to move it: no login that works but the one it's on.
fn offered<'a>(
    agent: AgentKind,
    on: Option<&AccountId>,
    accounts: impl Iterator<Item = &'a Account>,
    now_ms: i64,
) -> Option<Vec<Choice>> {
    let mut choices: Vec<Choice> = accounts
        .filter(|account| account.login_agent == Some(agent))
        .map(|account| Choice {
            id: account.id.clone(),
            called: crate::names::called(account).into(),
            works: account.works_for(agent, now_ms),
        })
        .filter(|choice| choice.works || on == Some(&choice.id))
        .collect();
    if !choices.iter().any(|choice| choice.works && on != Some(&choice.id)) {
        return None;
    }
    choices.sort_by_cached_key(|choice| (choice.called.to_lowercase(), choice.id.clone()));
    Some(choices)
}

impl WorkspacePane {
    /// What the menu offers for `session`, as [`offered`] says. `None` when there's nowhere else to move it, and from
    /// a hub from before it kept defaults, which can't move one. One that keeps defaults but is from before moves has
    /// it, and refuses the move, which the person is told.
    pub(super) fn moves(&self, session: &Session, cx: &Context<Self>) -> Option<Moves> {
        let all = self.shown.all();
        all.hub_settings()?.account_defaults.as_ref()?;
        let on = session.assigned_account.clone();
        let accounts = offered(session.agent, on.as_ref(), all.accounts(), look::now_ms(cx))?;
        let off = self.hub == Hub::Away;
        Some(Moves { session: session.id.clone(), accounts, on, off, pane: cx.weak_entity() })
    }

    /// Moves `session` to the account `to`. Refused while its agent is at work under another account, since moving
    /// stops it, so the person is asked, and it's moved anyway if they say to.
    fn move_chat(&mut self, session: SessionId, to: Choice, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (id, account) = (session.clone(), to.id.clone());
        let moving = self.client.start(move |client| client.session_assign_account(&id, &account, force));
        cx.spawn_in(window, async move |this, cx| {
            let moved = moving.recv().await;
            let _ = this.update_in(cx, |t, window, cx| match moved {
                Ok(session) => t.took_reply(session, cx),
                Err(ClientError::Api(e)) if e.code == ErrorCode::NeedsForce && !force => {
                    Self::confirm_forced_move(session, to, window, cx);
                }
                Err(e) => cx.emit(PaneEvent::Error(crate::failed::couldnt("move the chat", &e))),
            });
        })
        .detach();
    }

    /// Asks the person whether to stop the agent of `session`, which is working, to move the chat to the account `to`.
    fn confirm_forced_move(session: SessionId, to: Choice, window: &mut Window, cx: &mut Context<Self>) {
        let called = &to.called;
        let said = format!(
            "Its agent is working, and moving the chat stops it. It goes on under {called} once you send it something."
        );
        let title = format!("Move this chat to {called}?");
        let answer = window.prompt(PromptLevel::Warning, &title, Some(&said), &["Keep working", "Move anyway"], cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update_in(cx, |t, window, cx| t.move_chat(session, to, true, window, cx));
            }
        })
        .detach();
    }
}

/// The button that opens the menu, above it, which holds the card it's on open while the menu is, since the menu is
/// drawn outside the card.
pub(super) fn button(moves: &Moves, hold: &HoldOpen) -> AnyElement {
    let (moves, hold) = (moves.clone(), hold.clone());
    Button::new("account-move")
        .hand()
        .ghost()
        .sized(look::ControlSize::Small)
        .dropdown_caret(true)
        .label("Move")
        .off_while(moves.off)
        .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, window, cx| {
            // Built as it opens, so this is when it starts to hold the card.
            hold.while_open(&cx.entity(), window, cx);
            menu_of(&moves, menu)
        })
        .into_any_element()
}

/// The accounts, with a check on the one the chat is on. One whose login doesn't work is off, since the hub would
/// refuse it.
fn menu_of(moves: &Moves, menu: PopupMenu) -> PopupMenu {
    moves.accounts.iter().fold(menu, |menu, choice| {
        let (pane, session, to) = (moves.pane.clone(), moves.session.clone(), choice.clone());
        let chosen = moves.on.as_ref() == Some(&choice.id);
        menu.item(PopupMenuItem::new(choice.called.clone()).checked(chosen).disabled(!choice.works).on_click(
            move |_, window, cx| {
                if !chosen {
                    let to = to.clone();
                    let _ = pane.update(cx, |pane, cx| pane.move_chat(session.clone(), to, false, window, cx));
                }
            },
        ))
    })
}

#[cfg(test)]
mod tests {
    use protocol::model::HeldLogin;

    use super::*;

    const NOW: i64 = 1_000;

    /// A login added for `agent`, called `name`, which the hub holds as `held_login`, or doesn't, once disconnected.
    fn added(id: &str, name: &str, agent: AgentKind, held_login: Option<HeldLogin>) -> Account {
        Account {
            name: Some(name.to_string()),
            login_agent: Some(agent),
            held_login,
            ..protocol::fixtures::account(id)
        }
    }

    /// A login the hub holds that ends at `ends_at_ms`, or never.
    fn ending(ends_at_ms: Option<i64>) -> HeldLogin {
        HeldLogin { connected_at_ms: 0, ends_at_ms }
    }

    /// What's offered, as (what it's called, whether it works), of `accounts` for a Claude chat on `on`.
    fn offered_to(on: Option<&str>, accounts: &[Account]) -> Option<Vec<(String, bool)>> {
        let on = on.map(AccountId::from);
        let choices = offered(AgentKind::Claude, on.as_ref(), accounts.iter(), NOW)?;
        Some(choices.into_iter().map(|choice| (choice.called.to_string(), choice.works)).collect())
    }

    #[test]
    fn a_chat_is_offered_the_logins_that_work_and_the_one_its_on_only_when_it_could_go_elsewhere() {
        let work = added("work", "Work", AgentKind::Claude, Some(ending(None)));
        let personal = added("personal", "Personal", AgentKind::Claude, Some(ending(Some(NOW + 1))));
        let ended = added("ended", "Ended", AgentKind::Claude, Some(ending(Some(NOW))));
        let gone = added("gone", "Gone", AgentKind::Claude, None);
        let codex = added("codex", "Codex", AgentKind::Codex, Some(ending(None)));
        let all = [work.clone(), personal, ended.clone(), gone, codex.clone()];
        let both = |work: bool| Some(vec![("Personal".to_string(), true), ("Work".to_string(), work)]);

        assert_eq!(offered_to(Some("work"), &all), both(true), "the others that work, in order, and its own");
        assert_eq!(
            offered_to(None, std::slice::from_ref(&work)),
            Some(vec![("Work".to_string(), true)]),
            "a chat on none"
        );
        assert_eq!(offered_to(Some("work"), &[work.clone(), ended.clone(), codex]), None, "on the only one that works");
        let on_ended = offered_to(Some("ended"), &[work, ended]);
        assert_eq!(on_ended, Some(vec![("Ended".to_string(), false), ("Work".to_string(), true)]), "its own, off");
        assert_eq!(offered_to(None, &[]), None, "none added");
    }
}
