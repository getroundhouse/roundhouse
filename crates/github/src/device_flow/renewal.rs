use protocol::client::ApiError;
use serde::Serialize;

use super::super::Github;
use super::super::client::{unavailable, within};
use super::approval::{GithubRefreshToken, Given, Granted, granted};
use super::refusal::DeviceFlowError;

/// The grant a sign-in's token is renewed under, as OAuth names it.
const REFRESH_GRANT: &str = "refresh_token";

/// What GitHub says of what renews a token when it won't take it.
const BAD_REFRESH_TOKEN: &str = "bad_refresh_token";

/// What renewing a sign-in's token came to, as GitHub answered.
#[derive(Debug)]
pub enum Renewal {
    /// A token, and what renews it, in place of those it was asked with, which GitHub revoked as it gave these.
    Renewed(Granted),
    /// GitHub refused what renews the token, as it does one that has ended, been revoked or been used already, as the
    /// message says, which only signing in again gets past.
    Refused(String),
}

/// What `POST /login/oauth/access_token` is sent, to renew a sign-in's token. The app's client ID is all it takes, with
/// no client secret, as a real renewal showed in November 2026.
#[derive(Serialize)]
struct Renewing<'a> {
    client_id: &'a str,
    grant_type: &'a str,
    refresh_token: &'a str,
}

impl Github {
    /// Renews a sign-in's token with `refresh`, what renews it, as `POST /login/oauth/access_token` does, within
    /// [`ASK_WITHIN`](super::super::ASK_WITHIN). GitHub revokes the token and `refresh` as it gives what replaces them,
    /// so what it gives has to be kept before anything relies on it. Refused with `Unavailable` for whatever may pass
    /// by the next try: GitHub can't be reached, doesn't answer in time, answers with a status other than a 2xx, as
    /// when it's limiting how often it's asked, or with neither a token nor `bad_refresh_token`. *Unconfirmed:* trying
    /// again after every answer but a token and `bad_refresh_token`.
    pub async fn renew(&self, refresh: &GithubRefreshToken) -> Result<Renewal, ApiError> {
        let asking = Renewing {
            client_id: self.config.client_id.as_str(),
            grant_type: REFRESH_GRANT,
            refresh_token: refresh.as_str(),
        };
        let (status, body) = within(self.post_to_login("/login/oauth/access_token", &asking)).await?;
        if !(200..300).contains(&status) {
            return Err(unavailable(format!("GitHub answered {status} when asked to renew a sign-in's token")));
        }
        if let Ok(given) = serde_json::from_str::<Given>(&body) {
            return Ok(Renewal::Renewed(granted(given)));
        }
        match serde_json::from_str::<DeviceFlowError>(&body) {
            Ok(said) if said.error == BAD_REFRESH_TOKEN => Ok(Renewal::Refused(said.said())),
            // Such as `incorrect_client_credentials` or `unsupported_grant_type`, which say nothing of what renews
            // the token, so signing in again needn't get past them, and it's tried again with the same.
            Ok(said) => Err(unavailable(format!("asked to renew a sign-in's token, {}", said.said()))),
            Err(_) => Err(unavailable("GitHub answered with neither a token nor an error when asked to renew one")),
        }
    }
}
