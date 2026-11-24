//! Asking a hub installed as a service to update itself to the release it found, from the card over the build line.

use gpui_kit::{AppContext as _, Context};
use protocol::client::admin::HubUpdateTo;
use protocol::model::Rev;

use super::{AppView, Updating};

/// The app's own request to update the hub it's connected to, which only this client made, so the app keeps it. How
/// an update the hub took goes is the hub's record, which every client shows. A new connection forgets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HubUpdateAsk {
    /// The app asked the hub to update to `next` when its record was at `rev`. The card says it's updating, so it isn't
    /// asked twice, until the hub says the update failed after that.
    Asked { next: String, rev: Rev },
    /// The hub refused to update to `next`, for `why`, which the card says until it's asked again.
    Refused { next: String, why: String },
}

impl AppView {
    /// Has the hub update to the release the card shows. Once it has started, its record says it's updating, which
    /// the card follows, and every other client's with it. Why it refused, if it does, goes on the card.
    pub fn update_hub(&mut self, cx: &mut Context<Self>) {
        let Some(Updating::Hub { next, updating: false, .. }) = self.build_line(cx).update else { return };
        let Some(client) = self.client() else { return };
        let rev = self.records.hub_update().map_or(Rev::default(), |found| found.rev);
        self.hub_update_asked = Some(HubUpdateAsk::Asked { next: next.clone(), rev });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let version = next.clone();
            let asked = cx.background_spawn(async move { client.call(HubUpdateTo { version }) }).await;
            let Err(e) = asked else { return };
            let why = client_core::refusals::reason(&e);
            let _ = this.update(cx, |this, cx| {
                this.hub_update_asked = Some(HubUpdateAsk::Refused { next, why });
                cx.notify();
            });
        })
        .detach();
    }
}
