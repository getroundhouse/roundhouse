use std::ops::Not;

use protocol::model::{ToolScopes, ToolSignInId, ToolSignInStatus};
use protocol::worker::TokenNumber;
use serde::{Deserialize, Serialize};
use tool_sign_in::{AccessToken, Granted, RefreshToken, Registration, Renewable, SignedIn};

use crate::store::later_ms;

/// A sign-in to a tool server, as its file holds it, whole.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(super) struct KeptSignIn {
    /// The sign-in a person approved last, which finishing it again is answered by.
    pub(super) made_by: ToolSignInId,
    pub(super) renewable: Renewable,
    /// The registration it was made with, which signing in again uses. `None` where it was made with the client document,
    /// and once its authorization server has said it knows no such client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) registration: Option<Registration>,
    pub(super) number: TokenNumber,
    /// `None` once it's gone, after which only the person signing in again brings it back, with what it was asked to
    /// allow.
    pub(super) tokens: Option<KeptTokens>,
    /// What its server refused a call for the lack of, with a token of this sign-in, for signing in again to ask for. Empty
    /// when none was.
    #[serde(default, skip_serializing_if = "ToolScopes::is_empty")]
    pub(super) wants: ToolScopes,
    /// Whether it was signed in to only for what the sign-in before it had been asked to allow as well, and which its
    /// server wanted all the same. The person may have left that out the first time, so it's asked for again, and that's
    /// once: a server that still wants what this one was asked for has nothing more to be given. *Unconfirmed.*
    #[serde(default, skip_serializing_if = "Not::not")]
    pub(super) asked_again: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(super) struct KeptTokens {
    pub(super) access: AccessToken,
    /// When the access token ends, by the hub's clock, or `None` where its authorization server didn't say, so it's used
    /// until it's refused.
    pub(super) ends_at_ms: Option<i64>,
    /// `None` where nothing renews it.
    pub(super) refresh: Option<RefreshToken>,
}

impl KeptSignIn {
    /// What a person `signed_in` as `made_by`, given at `given_at_ms`, in place of `before`, if the server was signed in
    /// to before.
    pub(super) fn new(
        made_by: ToolSignInId,
        signed_in: SignedIn,
        given_at_ms: i64,
        before: Option<&KeptSignIn>,
    ) -> Self {
        let SignedIn { renewable, granted, registration } = signed_in;
        let number = before.map_or(TokenNumber::FIRST, |before| before.number.next());
        // What was wanted at another authorization server is nothing to this one.
        let before = before.filter(|before| before.renewable.issuer() == renewable.issuer());
        // What was wanted after this one was asked for is still to ask for.
        let wants = before.map_or_default(|before| before.wants.clone().without(renewable.scopes()));
        let asked_again = before.is_some_and(KeptSignIn::would_ask_again);
        let tokens = Some(KeptTokens::given(granted, given_at_ms));
        Self { made_by, renewable, registration, number, tokens, wants, asked_again }
    }

    /// Whether signing in again asks for no more than this one was asked for, which its server wanted all the same. One
    /// nothing has been wanted of is what the one before it made it.
    fn would_ask_again(&self) -> bool {
        if self.wants.is_empty() { self.asked_again } else { self.renewable.scopes().has_all(&self.wants) }
    }

    /// Whether its server `wants` only what has been asked for again already, so asking for it once more changes nothing.
    pub(super) fn wants_in_vain(&self, wants: &ToolScopes) -> bool {
        self.asked_again && self.renewable.scopes().has_all(wants)
    }

    /// The same, renewed for `granted`, given at `given_at_ms`.
    pub(super) fn renewed(self, granted: Granted, given_at_ms: i64) -> Self {
        Self { number: self.number.next(), tokens: Some(KeptTokens::given(granted, given_at_ms)), ..self }
    }

    /// The same, gone.
    pub(super) fn gone(self) -> Self {
        Self { tokens: None, ..self }
    }

    /// The same, with no registration for signing in again.
    pub(super) fn unregistered(self) -> Self {
        Self { registration: None, ..self }
    }

    /// What the server's record says of it.
    pub(super) fn status(&self) -> ToolSignInStatus {
        match self.tokens {
            Some(_) if self.wants.is_empty() => ToolSignInStatus::Works,
            Some(_) => ToolSignInStatus::NeedsMoreAccess,
            None => ToolSignInStatus::NeedsSignIn,
        }
    }
}

impl KeptTokens {
    fn given(granted: Granted, given_at_ms: i64) -> Self {
        let Granted { access, lasts, refresh } = granted;
        Self { access, ends_at_ms: lasts.map(|lasts| later_ms(given_at_ms, lasts)), refresh }
    }
}
