use std::time::Duration;

use protocol::client::ApiError;
use protocol::model::GithubToken;
use serde::{Deserialize, Serialize};

use super::super::Github;
use super::super::client::unavailable;
use super::code::DeviceCode;
use super::pacing::{Asked, paced};
use super::refusal::DeviceFlowError;

/// The grant a device code's token is asked for under, as OAuth's device flow names it.
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

protocol::tokens!(
    /// What GitHub gives a sign-in beside its token, which renews the token without the person until this ends too.
    /// It's kept in the sign-in's login file, and sent nowhere but GitHub.
    GithubRefreshToken,
);

/// How a sign-in ended, as GitHub said, or by the clock.
#[derive(Debug)]
pub(crate) enum Approval {
    /// The person approved it, and this is what GitHub gave.
    Granted(Granted),
    /// The person refused it on GitHub.
    Refused,
    /// Its code ended first.
    Ended,
    /// GitHub answered with something the hub can't go on from, as when the app doesn't let people sign in through the
    /// device flow, as the message says.
    Failed(String),
}

/// What GitHub gives a sign-in once the person approves it.
#[derive(Debug)]
pub(crate) struct Granted {
    pub(crate) token: GithubToken,
    /// How long the token lasts from when GitHub gave it, or `None` for one that doesn't end, as an app whose tokens
    /// don't end gives.
    pub(crate) lasts: Option<Duration>,
    /// What renews the token, with how long it lasts, given with a token that ends.
    pub(crate) refresh: Option<(GithubRefreshToken, Option<Duration>)>,
}

/// What `POST /login/oauth/access_token` is sent, to ask after a device code's token.
#[derive(Serialize)]
struct Asking<'a> {
    client_id: &'a str,
    device_code: &'a str,
    grant_type: &'a str,
}

/// A token, as `POST /login/oauth/access_token` grants one.
#[derive(Deserialize)]
struct Given {
    access_token: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    refresh_token_expires_in: Option<u64>,
}

impl Github {
    /// Asks GitHub after `code`'s token, first and between each ask as long as GitHub says, longer after it says
    /// `slow_down`, until the person approves or refuses, or the code ends, by GitHub's word or by the clock, as
    /// [`paced`] says. Dropping it stops asking.
    pub(crate) async fn wait_for_approval(&self, code: &DeviceCode) -> Approval {
        paced(code.interval, code.ends, || self.ask_after_token(code)).await
    }

    /// What one ask after `code`'s token comes to, as GitHub answers it, without a limit of its own.
    pub(super) async fn ask_after_token(&self, code: &DeviceCode) -> Result<Asked, ApiError> {
        let asking = Asking {
            client_id: self.config.client_id.as_str(),
            device_code: code.device.as_str(),
            grant_type: DEVICE_GRANT,
        };
        let (status, body) = self.post_to_login("/login/oauth/access_token", &asking).await?;
        if !(200..300).contains(&status) {
            return Err(unavailable(format!("GitHub answered {status} when asked after a sign-in's token")));
        }
        if let Ok(given) = serde_json::from_str::<Given>(&body) {
            return Ok(Asked::Done(Approval::Granted(granted(given))));
        }
        let Ok(said) = serde_json::from_str::<DeviceFlowError>(&body) else {
            return Err(unavailable("GitHub answered with neither a token nor an error when asked after a sign-in's"));
        };
        Ok(match said.error.as_str() {
            "authorization_pending" => Asked::Again,
            "slow_down" => Asked::SlowDown { interval: said.interval.map(Duration::from_secs) },
            "expired_token" => Asked::Done(Approval::Ended),
            "access_denied" => Asked::Done(Approval::Refused),
            // Such as `device_flow_disabled`, `incorrect_client_credentials` or `incorrect_device_code`, none of which
            // asking again changes.
            _ => Asked::Done(Approval::Failed(said.said())),
        })
    }
}

fn granted(given: Given) -> Granted {
    let Given { access_token, expires_in, refresh_token, refresh_token_expires_in } = given;
    let refresh = refresh_token
        .map(|refresh| (GithubRefreshToken::new(refresh), refresh_token_expires_in.map(Duration::from_secs)));
    Granted { token: GithubToken::new(access_token), lasts: expires_in.map(Duration::from_secs), refresh }
}
