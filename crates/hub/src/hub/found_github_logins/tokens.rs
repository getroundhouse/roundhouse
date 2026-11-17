use std::collections::HashMap;
use std::time::Duration;

use protocol::model::ProviderName;
use protocol::worker::FoundToken;
use tokio::sync::watch;

/// The token each provider's machines' own `gh` uses, as the first of the provider's workers to say since the hub
/// started handed it, kept in memory alone, so the hub reads GitHub as each machine's own login while that's the
/// default, and only for that provider's workspaces. A provider whose worker said it had none, as one logged out does,
/// or a worker from before workers handed one, has `None`. One not answered for since the hub started has nothing.
pub(crate) struct FoundTokens(watch::Sender<HashMap<ProviderName, Option<FoundToken>>>);

impl Default for FoundTokens {
    fn default() -> Self {
        Self(watch::Sender::new(HashMap::new()))
    }
}

impl FoundTokens {
    /// What `provider`'s worker handed the hub.
    pub(in crate::hub) fn found(&self, provider: ProviderName, token: Option<FoundToken>) {
        self.0.send_modify(|tokens| {
            tokens.insert(provider, token);
        });
    }

    /// `provider`'s, once one of its workers has said, waiting `within` for one asked as it linked, or `None` when it
    /// had none, or none has said by then.
    pub(crate) async fn of(&self, provider: &ProviderName, within: Duration) -> Option<FoundToken> {
        let mut said = self.0.subscribe();
        let answered = tokio::time::timeout(within, said.wait_for(|tokens| tokens.contains_key(provider))).await;
        answered.ok()?.ok()?.get(provider).cloned().flatten()
    }

    /// Forgets that `provider`'s worker handed none, if it did, saying whether it had, so it's waited for again.
    pub(crate) fn forget_none(&self, provider: &ProviderName) -> bool {
        self.0.send_if_modified(|tokens| {
            let none = tokens.get(provider).is_some_and(Option::is_none);
            if none {
                tokens.remove(provider);
            }
            none
        })
    }

    /// Forgets `provider`'s, as when GitHub refused it, so it's waited for again.
    pub(crate) fn forget(&self, provider: &ProviderName) {
        self.0.send_modify(|tokens| {
            tokens.remove(provider);
        });
    }
}
