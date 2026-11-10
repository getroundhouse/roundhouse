//! How full the agent's context is: a ring beside Send, and while the pointer rests on it, a card that says what the
//! context holds, how much of the account's limits is left, and whose account it is.

use std::f32::consts::TAU;

use client_core::ClientError;
use gpui_kit::base::plot::shape::{Arc, ArcData};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::{
    Anchor, App, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, canvas, div, px,
};
use protocol::model::{SessionAccount, SessionId};

use super::WorkspacePane;
use super::composer::{Slot, middle};
use crate::hover::{CLOSES_AFTER, OPENS_AFTER};

mod card;
mod words;

/// As wide as a mark beside text, so it sits with the composer's other small things.
const RING: Pixels = px(16.);
/// How thick the ring's line is.
const LINE: f32 = 2.;

/// What the hub last said of the account a chat's agent runs under.
#[derive(Default)]
pub(super) struct AccountRead {
    /// What it said, and of which chat, since chats can run under different accounts. `None` until the hub has said,
    /// once it has failed to, and from a hub that can't.
    known: Option<(SessionId, SessionAccount)>,
    /// A read is under way, so another hover doesn't start a second.
    reading: bool,
    /// The hub said it doesn't know the request, so it isn't asked again on this connection.
    unsupported: bool,
}

impl AccountRead {
    /// What's known of the account `session`'s agent runs under.
    fn of(&self, session: &SessionId) -> Option<&SessionAccount> {
        self.known.as_ref().filter(|(of, _)| of == session).map(|(_, account)| account)
    }

    /// Takes what the hub answered about `session`. A hub that couldn't say leaves nothing known, rather than what an
    /// earlier read said, which may no longer be so.
    fn answered(&mut self, session: SessionId, answer: Result<SessionAccount, ClientError>) {
        self.reading = false;
        self.unsupported = answer.as_ref().is_err_and(ClientError::is_unsupported);
        self.known = answer.ok().map(|account| (session, account));
    }
}

impl WorkspacePane {
    /// The ring for the chat showing, once its agent has said how full its context is. Until then there's nothing
    /// true to draw. It's in muted text's colour however full it is, since amber and red already mean something.
    ///
    /// The card opens above the ring's slot, with its right edge on the slot's, so it covers neither the ring nor
    /// Send. It's made from what the pane holds each time the pane is drawn, so it follows the agent while it shows,
    /// and fills in the account once the hub has said.
    pub(super) fn context_ring(&self, cx: &mut Context<Self>) -> Option<Slot> {
        let session = self.session()?;
        let (context, agent) = (session.context.clone()?, session.agent);
        let account = self.account.of(&session.id).cloned();
        let this = cx.weak_entity();
        let used = context.used_fraction();
        // The whole slot is the ring's to point at, and what the card lines up with, so the ring's own square is as
        // big as the slot.
        let ring = middle().size(Slot::SIZE).child(div().size(RING).child(ring(used, cx)));
        let hover = HoverCard::new("context-hover")
            .appearance(false)
            .anchor(Anchor::BottomRight)
            .open_delay(OPENS_AFTER)
            .close_delay(CLOSES_AFTER)
            .trigger(ring)
            .content(move |_, _, cx| card::card(&context, agent, account.as_ref(), cx))
            .on_open_change(move |open, _, cx| {
                if !*open {
                    return;
                }
                // The account is read each time the card shows, since its limits move as agents work. Deferred, so
                // it's never inside whatever told the card to open, which the kit doesn't promise is outside the
                // pane's own update.
                let pane = this.clone();
                cx.defer(move |cx| {
                    let _ = pane.update(cx, Self::read_account);
                });
            });
        // Said in words too, since the ring alone says nothing to someone who can't see it.
        let said = format!("Context {} used", words::percent(used));
        Some(Slot::holding(div().id("context").role(Role::Status).aria_label(said).child(hover).test_support()))
    }

    /// Forgets what the hub said of the account, on a new connection, which may be to a hub that says otherwise.
    pub(super) fn forget_account(&mut self) {
        self.account = AccountRead::default();
    }

    /// Asks the hub whose account the showing chat's agent runs under. A hub that doesn't know the request is asked
    /// once, and the card is then only what the context holds.
    fn read_account(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.active.clone() else { return };
        if self.account.unsupported || std::mem::replace(&mut self.account.reading, true) {
            return;
        }
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let asked = session.clone();
            let answer = cx.background_spawn(async move { client.session_account(&asked) }).await;
            let _ = this.update(cx, |t, cx| {
                t.account.answered(session, answer);
                cx.notify();
            });
        })
        .detach();
    }
}

/// A ring filled clockwise from the top by `used`, from 0 to 1, over a fainter whole ring. It's drawn at once, with
/// nothing moving, since only a working agent moves anything.
fn ring(used: f32, cx: &App) -> impl IntoElement {
    let color = cx.theme().muted_foreground;
    canvas(
        |bounds: Bounds<Pixels>, _: &mut Window, _: &mut App| bounds,
        move |_, bounds, window, _| {
            let outer = bounds.size.width.min(bounds.size.height).as_f32() / 2.;
            let arc = Arc::new().inner_radius(outer - LINE).outer_radius(outer);
            arc.paint(&ArcData::new(&(), 0, 1., 0., TAU), color.opacity(0.25), &bounds, window);
            if used > 0. {
                arc.paint(&ArcData::new(&(), 1, used, 0., used * TAU), color, &bounds, window);
            }
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use protocol::client::{ApiError, ErrorCode};
    use protocol::model::AgentAccount;

    use super::*;

    fn account(email: &str) -> SessionAccount {
        let account = AgentAccount { email: Some(email.to_string()), ..AgentAccount::default() };
        SessionAccount { account, limits: Vec::new() }
    }

    #[test]
    fn an_account_is_known_only_for_the_chat_it_was_read_for() {
        let mut read = AccountRead::default();

        read.answered("a".into(), Ok(account("ada@example.com")));

        assert_eq!(read.of(&"a".into()), Some(&account("ada@example.com")));
        assert_eq!(read.of(&"b".into()), None, "another chat can run under another account");
    }

    #[test]
    fn a_hub_that_couldnt_say_leaves_nothing_known_rather_than_what_it_said_before() {
        let mut read = AccountRead::default();
        read.answered("a".into(), Ok(account("ada@example.com")));

        read.answered("a".into(), Err(ClientError::Disconnected));

        assert_eq!(read.of(&"a".into()), None);
        assert!(!read.unsupported, "it can be asked again");
    }

    #[test]
    fn a_hub_that_doesnt_know_the_request_isnt_asked_again() {
        let mut read = AccountRead { reading: true, ..AccountRead::default() };

        read.answered("a".into(), Err(ApiError::new(ErrorCode::Unsupported, "no such request").into()));

        assert!(read.unsupported && !read.reading);
    }
}
