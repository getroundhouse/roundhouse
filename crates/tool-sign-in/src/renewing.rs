use protocol::model::ToolScopes;
use serde::{Deserialize, Serialize};

use crate::fetch::json;
use crate::token_endpoint::{Refusal, granted};
use crate::{Failure, Granted, Issuer, OauthError, RefreshToken, ServerUrl, ToolSignIn};

/// What OAuth calls a refresh token that has ended, been used or been revoked, and a client the authorization server
/// no longer takes, by RFC 6749, section 5.2.
const FOR_GOOD: [&str; 2] = ["invalid_grant", "invalid_client"];

/// What renews a sign-in, with its refresh token. It has no secret in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Renewable {
    pub(crate) server: ServerUrl,
    /// The server, as its metadata named it.
    pub(crate) resource: String,
    pub(crate) issuer: Issuer,
    pub(crate) token_endpoint: String,
    pub(crate) client_id: String,
    pub(crate) scopes: ToolScopes,
}

/// What renewing a sign-in came to, as its authorization server answered.
#[derive(Debug)]
pub enum Renewal {
    /// Tokens in place of those it had, which have to be kept before anything relies on them, since the refresh token
    /// may be a new one, with the last refused from then on.
    Renewed(Granted),
    /// The authorization server refused it for good, so only signing in again gets past that.
    SignInAgain(OauthError),
}

impl Renewable {
    /// The authorization server it's a sign-in with.
    pub fn issuer(&self) -> &Issuer {
        &self.issuer
    }

    /// What it was asked to be allowed, which signing in again asks for too.
    pub fn scopes(&self) -> &ToolScopes {
        &self.scopes
    }
}

impl ToolSignIn {
    /// Renews a sign-in with `refresh`, what it has now, which only one call at a time may be given. A failure may pass,
    /// and is no reason to sign in again: the authorization server couldn't be reached, didn't answer in time, answered
    /// with a 429 or a 5xx, or answered with neither tokens nor a refusal for good. *Unconfirmed:* trying again after
    /// every answer but tokens, `invalid_grant` and `invalid_client`.
    pub async fn renew(&self, renewable: &Renewable, refresh: &RefreshToken) -> Result<Renewal, Failure> {
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", &renewable.client_id),
            ("resource", &renewable.resource),
        ];
        let calls = self.calls_for(&renewable.server).await?;
        let answer = calls.post_form(&calls.address(&renewable.token_endpoint)?, &form).await?;
        if let Some(Refusal { error }) = json(&answer).filter(|_| answer.refused())
            && FOR_GOOD.contains(&error.as_str())
        {
            return Ok(Renewal::SignInAgain(error));
        }
        let mut granted = granted(answer)?;
        // One that isn't replaced is still good, by RFC 6749, section 6.
        granted.refresh.get_or_insert_with(|| refresh.clone());
        Ok(Renewal::Renewed(granted))
    }
}
