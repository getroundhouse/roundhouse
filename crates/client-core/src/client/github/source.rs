//! Following what the hub says of GitHub: one subscription, which says all that every view shows, again under the same
//! ID each time that changes.

use parking_lot::Mutex;
use protocol::client::github::{GithubShown, GithubSubscribe};

use super::held::GithubHeld;
use crate::client::follow::Source;
use crate::client::{Client, ClientError, Result, Subscription, SubscriptionItem};

/// The subscription on this connection, and what it last said is shown.
#[derive(Default)]
pub(super) struct GithubSource {
    subscription: Option<u64>,
    said: Option<GithubShown>,
}

impl Source for GithubSource {
    type Held = GithubHeld;

    fn resume(&mut self, client: &Client, held: &Mutex<GithubHeld>) -> Result<Subscription> {
        self.subscription = None;
        let shown = held.lock().shown();
        let subscribed = client.subscribe(|subscription_id| GithubSubscribe { subscription_id, shown: shown.clone() });
        let unsupported = subscribed.as_ref().is_err_and(ClientError::is_unsupported);
        {
            let mut held = held.lock();
            if held.unsupported != unsupported {
                held.unsupported = unsupported;
                held.tell_all();
            }
        }
        let subscription = subscribed?;
        self.subscription = Some(subscription.id);
        self.said = Some(shown);
        Ok(subscription)
    }

    fn apply(&mut self, _: &Client, held: &Mutex<GithubHeld>, item: SubscriptionItem) -> Result<bool> {
        let SubscriptionItem::Github(update) = item else { return Ok(false) };
        Ok(held.lock().apply(update))
    }

    /// Tells the hub what's shown, if it has changed since it last said, and lets go of what no view shows now.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<GithubHeld>, _: &dyn Fn()) -> Result<bool> {
        let shown = {
            let mut held = held.lock();
            held.forget_unshown();
            held.shown()
        };
        let (Some(client), Some(subscription_id)) = (connection, self.subscription) else { return Ok(false) };
        if self.said.as_ref() == Some(&shown) {
            return Ok(false);
        }
        client.call(GithubSubscribe { subscription_id, shown: shown.clone() })?;
        self.said = Some(shown);
        Ok(false)
    }

    fn reconnected(&mut self) {
        *self = Self::default();
    }

    /// A hub too old to say what GitHub says, which [`GithubHeld::unsupported`] says.
    fn does_without(&self, error: &ClientError) -> bool {
        error.is_unsupported()
    }
}
