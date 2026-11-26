use protocol::model::{ToolSignInId, ToolSignInStatus};
use protocol::worker::TokenNumber;
use serde::{Deserialize, Serialize};
use tool_sign_in::{AccessToken, Granted, RefreshToken, Renewable, SignedIn};

use crate::store::later_ms;

/// A sign-in to a tool server, as its file holds it, whole.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(super) struct KeptSignIn {
    /// The sign-in a person approved last, which finishing it again is answered by.
    pub(super) made_by: ToolSignInId,
    pub(super) renewable: Renewable,
    pub(super) number: TokenNumber,
    /// `None` once it's gone, after which only the person signing in again brings it back, with what it was asked to
    /// allow.
    pub(super) tokens: Option<KeptTokens>,
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
        let SignedIn { renewable, granted } = signed_in;
        let number = before.map_or(TokenNumber::FIRST, |before| before.number.next());
        Self { made_by, renewable, number, tokens: Some(KeptTokens::given(granted, given_at_ms)) }
    }

    /// The same, renewed for `granted`, given at `given_at_ms`.
    pub(super) fn renewed(self, granted: Granted, given_at_ms: i64) -> Self {
        Self { number: self.number.next(), tokens: Some(KeptTokens::given(granted, given_at_ms)), ..self }
    }

    /// The same, gone.
    pub(super) fn gone(self) -> Self {
        Self { tokens: None, ..self }
    }

    /// What the server's record says of it.
    pub(super) fn status(&self) -> ToolSignInStatus {
        match self.tokens {
            Some(_) => ToolSignInStatus::Works,
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
