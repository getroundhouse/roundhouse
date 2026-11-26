//! The sign-in a hub's view has, if it has one: starting it, following it, opening its page again and putting it away.

use client_core::tool_sign_ins::{Begun, begun, couldnt_finish, couldnt_start};
use client_core::{Client, ClientError, new_tool_sign_in_id};
use gpui_kit::Context;
use protocol::client::tool_sign_ins::{
    REDIRECT_PORTS, SIGN_IN_WAITS, ToolSignInBegun, ToolSignInPage, ToolSignInReturned, redirect,
};
use protocol::model::{ToolServer, ToolSignInId};

use super::page::SignInPage;
use super::under_way::{Finishing, SignInTo, Stands, Starting, UnderWay, Waiting};
use super::{sent_back, words};
use crate::loopback::{Bound, Ended};
use crate::timers::Timers;

/// Signing in to the hub's MCP servers from this device, one at a time.
#[derive(Default)]
pub(crate) struct ToolSigningIn {
    /// The hub to ask, while there's one.
    hub: Option<Client>,
    /// How many connections to it there have been, which says which one something was sent on.
    connection: u64,
    under_way: Option<UnderWay>,
}

impl ToolSigningIn {
    /// The sign-in there is, from its start until it has signed in or it's put away, and how it stands.
    pub(crate) fn under_way(&self) -> Option<&UnderWay> {
        self.under_way.as_ref()
    }

    /// Signs in to what `to` says, in place of any sign-in there is, which is put away.
    pub(crate) fn start(&mut self, to: SignInTo, cx: &mut Context<Self>) {
        self.put_away(cx);
        let id = new_tool_sign_in_id();
        let stands = match self.ask_to_start(&id, &to, cx) {
            Ok(starting) => Stands::Starting(starting),
            Err(why) => Stands::Failed(why),
        };
        self.under_way = Some(UnderWay { id, to, stands });
        cx.notify();
    }

    /// Opens the page again, as the browser was first opened on it, while it waits there.
    pub(crate) fn open_page(&self, cx: &mut Context<Self>) {
        if let Some(UnderWay { stands: Stands::Waiting(waiting), .. }) = &self.under_way {
            cx.open_url(waiting.page.url());
        }
    }

    /// Ends the sign-in there is, having the hub forget it where it may have it waiting. Nothing listens from then on.
    pub(crate) fn put_away(&mut self, cx: &mut Context<Self>) {
        let Some(ended) = self.under_way.take() else { return };
        if ended.stands.may_wait() {
            self.cancel_at_hub(ended.id);
        }
        cx.notify();
    }

    /// The hub to ask from now on is `hub`, if there's one. What the person was sent back with, which the last
    /// connection lost, is sent on this one.
    pub(crate) fn hub_is(&mut self, hub: Option<Client>, cx: &mut Context<Self>) {
        if self.hub.is_none() && hub.is_some() {
            self.connection += 1;
        }
        self.hub = hub;
        self.finish(cx);
    }

    /// Has the hub start the sign-in `id`, with the person sent back to a listener of this device's, or says why it
    /// can't be asked.
    fn ask_to_start(&self, id: &ToolSignInId, to: &SignInTo, cx: &mut Context<Self>) -> Result<Starting, String> {
        let client = self.hub.clone().ok_or(words::NO_HUB)?;
        let bound = Bound::first_free(&REDIRECT_PORTS).map_err(|e| words::couldnt_listen(&e))?;
        let (asked, to, sent_back_to) = (id.clone(), to.asked(), redirect(bound.address().port()));
        let starting = client.start(move |client| client.tool_sign_in_start(&asked, to, sent_back_to));
        let id = id.clone();
        // Not with the sign-in, so a start that's answered after it was put away is still cancelled.
        cx.spawn(async move |this, cx| {
            let answer = starting.recv().await;
            let _ = this.update(cx, |this, cx| this.started(id, answer, cx));
        })
        .detach();
        Ok(Starting { bound })
    }

    /// Takes in what the hub answered the start of `id` with: opens its page, or says why there's none.
    fn started(&mut self, id: ToolSignInId, answer: client_core::Result<ToolSignInBegun>, cx: &mut Context<Self>) {
        let Some(UnderWay { to, stands, .. }) = self.under_way.take_if(|under_way| under_way.id == id) else {
            // Put away before the hub answered, which may have been before the hub had it to forget.
            if matches!(answer, Ok(ToolSignInBegun::Page { .. })) {
                self.cancel_at_hub(id);
            }
            return;
        };
        let Stands::Starting(Starting { bound }) = stands else { return };
        let stands = match answer.map(begun) {
            Ok(Begun::Page(page)) => self.wait(&id, &page, bound, cx),
            Ok(Begun::NoPage(no_page)) => Stands::NoWay(no_page),
            Err(e) => Stands::Failed(couldnt_start(&e)),
        };
        self.under_way = Some(UnderWay { id, to, stands });
        cx.notify();
    }

    /// Opens `page` in the browser and listens on `bound` for the person to be sent back, or says why not, having the
    /// hub forget `id`.
    fn wait(&self, id: &ToolSignInId, page: &ToolSignInPage, bound: Bound, cx: &mut Context<Self>) -> Stands {
        let failed = |why: String| {
            self.cancel_at_hub(id.clone());
            Stands::Failed(why)
        };
        let Some(page) = SignInPage::checked(page) else { return failed(words::NO_PAGE.to_string()) };
        let listening = match sent_back::listen(bound, page.state().clone(), Timers::of(cx).tool_sign_in_sends_back) {
            Ok(listening) => listening,
            Err(e) => return failed(words::couldnt_listen(&e)),
        };
        cx.open_url(page.url());
        let (id, ended) = (id.clone(), listening.ended());
        let sent_back = cx.spawn(async move |this, cx| {
            let ended = ended.recv_async().await;
            // Held until the person has been sent back, the limit has passed, or the sign-in has gone.
            drop(listening);
            let _ = this.update(cx, |this, cx| this.sent_back(&id, ended.ok(), cx));
        });
        Stands::Waiting(Waiting { page, _sent_back: sent_back })
    }

    /// Takes in how the listener of the sign-in `id` ended: with what the person was sent back with, which the hub is
    /// given, or as its limit passed.
    fn sent_back(&mut self, id: &ToolSignInId, ended: Option<Ended<ToolSignInReturned>>, cx: &mut Context<Self>) {
        match ended {
            Some(Ended::Heard(returned)) => {
                let Some(under_way) = self.under_way.as_mut().filter(|under_way| under_way.id == *id) else { return };
                under_way.stands = Stands::Finishing(Finishing { returned, sent: None });
                self.finish(cx);
                cx.notify();
            }
            // Nothing heard is the listener let go of, which only the sign-in going does, with this wait.
            Some(Ended::TimedOut) | None => self.failed(id, words::timed_out(SIGN_IN_WAITS), cx),
        }
    }

    /// Gives the hub what the person was sent back with, unless it has been on this connection, or there's no hub to
    /// ask now, and then once there is. The hub answers the same again when it's sent twice.
    fn finish(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.hub.clone() else { return };
        let Some(UnderWay { id, stands: Stands::Finishing(finishing), .. }) = &mut self.under_way else { return };
        if finishing.sent.as_ref().is_some_and(|(connection, _)| *connection == self.connection) {
            return;
        }
        let (asked, returned) = (id.clone(), finishing.returned.clone());
        let answered = client.start(move |client| client.tool_sign_in_finish(&asked, returned));
        let id = id.clone();
        let answering = cx.spawn(async move |this, cx| {
            let answer = answered.recv().await;
            let _ = this.update(cx, |this, cx| this.finished(&id, answer, cx));
        });
        finishing.sent = Some((self.connection, answering));
    }

    /// Takes in what the hub answered the finish of `id` with. One the connection lost is sent again on the next.
    fn finished(&mut self, id: &ToolSignInId, answer: client_core::Result<ToolServer>, cx: &mut Context<Self>) {
        match answer {
            Ok(_) => {
                self.under_way.take_if(|under_way| under_way.id == *id);
                cx.notify();
            }
            Err(ClientError::Disconnected | ClientError::Io(_) | ClientError::Unreachable { .. }) => {}
            Err(e @ (ClientError::Api(_) | ClientError::FingerprintMismatch { .. } | ClientError::Unexpected(_))) => {
                self.failed(id, couldnt_finish(&e), cx);
            }
        }
    }

    /// The sign-in `id` came to nothing, as `why` says. The hub forgets it, in case it has it waiting still.
    fn failed(&mut self, id: &ToolSignInId, why: String, cx: &mut Context<Self>) {
        let Some(under_way) = self.under_way.as_mut().filter(|under_way| under_way.id == *id) else { return };
        under_way.stands = Stands::Failed(why);
        self.cancel_at_hub(id.clone());
        cx.notify();
    }

    /// Has the hub forget the sign-in `id`, with a hub to ask, and otherwise it does by itself once it has waited as
    /// long as one does.
    fn cancel_at_hub(&self, id: ToolSignInId) {
        if let Some(client) = &self.hub {
            // Not waited for: nothing here goes by it, and it runs to the end whatever the window does.
            drop(client.start(move |client| client.tool_sign_in_cancel(&id)));
        }
    }
}
