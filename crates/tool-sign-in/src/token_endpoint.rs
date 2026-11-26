//! What an authorization server's token endpoint answers, for a code or a renewal, as RFC 6749, section 5, has it.

use std::time::Duration;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::fetch::{Answer, json};
use crate::{AccessToken, Failure, OauthError, RefreshToken};

/// What a sign-in's tokens are.
#[derive(Debug)]
pub struct Granted {
    pub access: AccessToken,
    /// How long the access token lasts from when it was given, or `None` where the authorization server doesn't say.
    pub lasts: Option<Duration>,
    /// What renews it, from now on. `None` where it can't be renewed.
    pub refresh: Option<RefreshToken>,
}

#[derive(Deserialize)]
struct Given {
    access_token: AccessToken,
    token_type: String,
    #[serde(default)]
    expires_in: Option<Value>,
    #[serde(default)]
    refresh_token: Option<RefreshToken>,
}

/// Why it gave none.
#[derive(Deserialize)]
pub(crate) struct Refusal {
    #[serde(deserialize_with = "named")]
    pub(crate) error: OauthError,
}

fn named<'de, D: Deserializer<'de>>(deserializer: D) -> Result<OauthError, D::Error> {
    Ok(OauthError::new(&String::deserialize(deserializer)?))
}

/// The tokens `answer` gives.
pub(crate) fn granted(answer: Answer) -> Result<Granted, Failure> {
    if !answer.succeeded() {
        return Err(answer.failure());
    }
    let what = "no token that goes in an Authorization header";
    let given = json::<Given>(&answer).filter(|given| !given.access_token.as_str().is_empty());
    let given = given.filter(|given| given.token_type.eq_ignore_ascii_case("bearer"));
    let Some(Given { access_token, expires_in, refresh_token, .. }) = given else {
        return Err(Failure::Unreadable { host: answer.host, what });
    };
    let refresh = refresh_token.filter(|refresh| !refresh.as_str().is_empty());
    Ok(Granted { access: access_token, lasts: expires_in.as_ref().and_then(seconds), refresh })
}

/// How long `said` is, which is a number of seconds, and which some write as text or with a fraction. What it can't be
/// read from says nothing, rather than have the tokens beside it, which may be good once, thrown away.
fn seconds(said: &Value) -> Option<Duration> {
    let seconds = said.as_f64().or_else(|| said.as_str()?.trim().parse().ok())?;
    Duration::try_from_secs_f64(seconds).ok()
}
