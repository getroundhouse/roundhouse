use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::ProviderName;

/// The providers whose machines' own GitHub login the hub has found since it started, or is asking a worker for, so
/// each is asked once a start, and by one worker at a time.
#[derive(Default)]
pub(crate) struct ProvidersAsked(Mutex<HashSet<ProviderName>>);

impl ProvidersAsked {
    /// `provider`'s ask, unless it has been answered since the hub started, or is being asked.
    pub(super) fn ask(self: &Arc<Self>, provider: ProviderName) -> Option<Asking> {
        if !self.0.lock().insert(provider.clone()) {
            return None;
        }
        Some(Asking { asked: Arc::clone(self), provider, answered: false })
    }

    /// Whether `provider` has been answered, or is being asked.
    #[cfg(test)]
    pub(super) fn has(&self, provider: &ProviderName) -> bool {
        self.0.lock().contains(provider)
    }
}

/// One provider's ask. Let go of before it's [`Asking::answered`], however that happens, it leaves the provider to be
/// asked again, by the next link to it.
pub(super) struct Asking {
    asked: Arc<ProvidersAsked>,
    provider: ProviderName,
    answered: bool,
}

impl Asking {
    pub(super) fn provider(&self) -> &ProviderName {
        &self.provider
    }

    /// Its answer is recorded, so the provider isn't asked again until the hub starts again.
    pub(super) fn answered(mut self) {
        self.answered = true;
    }
}

impl Drop for Asking {
    fn drop(&mut self) {
        if !self.answered {
            self.asked.0.lock().remove(&self.provider);
        }
    }
}
