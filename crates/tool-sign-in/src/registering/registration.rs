use serde::{Deserialize, Serialize};

use crate::{ClientId, Issuer, RedirectUri};

/// What an authorization server knows Roundhouse as, once it's registered there. It has no secret in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
    pub(crate) client_id: ClientId,
    pub(crate) issuer: Issuer,
    /// Where it was registered to have a person sent back to, each letter for letter.
    pub(crate) redirect_uris: Vec<RedirectUri>,
}

impl Registration {
    /// Whether it's one to sign in with at `issuer`, with the person sent back to `redirect`.
    pub(super) fn is_for(&self, issuer: &Issuer, redirect: &RedirectUri) -> bool {
        self.issuer == *issuer && self.redirect_uris.contains(redirect)
    }
}
