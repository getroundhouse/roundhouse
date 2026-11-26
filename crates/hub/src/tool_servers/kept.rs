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
    /// What its authorization server last said it granted, for signing in again to ask for too. Empty where it hasn't
    /// said, which means what it was asked for, and in a file from a build that didn't keep it.
    #[serde(default, skip_serializing_if = "ToolScopes::is_empty")]
    pub(super) granted: ToolScopes,
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
        let asked_again = before.is_some_and(KeptSignIn::would_ask_again);
        let (tokens, granted) = KeptTokens::given(granted, given_at_ms);
        let granted = granted.unwrap_or_default();
        // What was wanted after this one was asked for is still to ask for, unless it was granted all the same.
        let wants = before.map_or_default(|before| before.wants.clone().without(renewable.scopes()).without(&granted));
        Self { made_by, renewable, registration, number, tokens: Some(tokens), granted, wants, asked_again }
    }

    /// What signing in again asks to be allowed, beyond what its server says then, so nothing that worked stops
    /// working. What was granted may be more than was asked for, such as what its authorization server grants when it's
    /// asked for nothing, and less, where the person left some of it out, which is still asked for.
    pub(super) fn to_ask_for(&self) -> ToolScopes {
        self.renewable.scopes().clone().and(&self.granted).and(&self.wants)
    }

    /// Whether its server wanted only what it had been asked for all the same. What was granted without being asked for
    /// doesn't count, since asking for that by name may change something. One nothing has been wanted of is what the
    /// one before it made it.
    fn would_ask_again(&self) -> bool {
        if self.wants.is_empty() { self.asked_again } else { self.renewable.scopes().has_all(&self.wants) }
    }

    /// Whether its server `wants` only what has been asked for again already, so asking for it once more changes nothing.
    pub(super) fn wants_in_vain(&self, wants: &ToolScopes) -> bool {
        self.asked_again && self.renewable.scopes().has_all(wants)
    }

    /// The same, renewed for `granted`, given at `given_at_ms`. One that doesn't say what it granted granted the same.
    pub(super) fn renewed(self, granted: Granted, given_at_ms: i64) -> Self {
        let (tokens, granted) = KeptTokens::given(granted, given_at_ms);
        let granted = granted.unwrap_or(self.granted);
        Self { number: self.number.next(), tokens: Some(tokens), granted, ..self }
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
    /// Those of `granted`, given at `given_at_ms`, and what it says they're allowed, which isn't theirs alone to keep,
    /// since it outlasts them.
    fn given(granted: Granted, given_at_ms: i64) -> (Self, Option<ToolScopes>) {
        let Granted { access, lasts, refresh, scopes } = granted;
        (Self { access, ends_at_ms: lasts.map(|lasts| later_ms(given_at_ms, lasts)), refresh }, scopes)
    }
}
