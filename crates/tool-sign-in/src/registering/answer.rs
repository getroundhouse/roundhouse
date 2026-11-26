//! What an authorization server answers a client that asks to be registered, as RFC 7591, section 3.2, has it.

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::fetch::{Answer, json};
use crate::token_endpoint::Refusal;
use crate::{ClientId, Failure, OauthError};

/// How a client with no secret says who it is for tokens.
pub(crate) const NO_SECRET: &str = "none";

/// What came of asking.
pub(super) enum Answered {
    Registered(ClientId),
    /// It registered a client, of which it wants a secret, and Roundhouse is none.
    WithSecret,
    /// It won't register Roundhouse, for this of OAuth's errors where it says one.
    Refused(Option<OauthError>),
}

/// What's read of a registration. A secret in it is read only for whether it's there, and a token to change it with
/// is left unread.
#[derive(Deserialize)]
struct Given {
    client_id: String,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    client_secret: Option<IgnoredAny>,
}

pub(super) fn answered(answer: Answer) -> Result<Answered, Failure> {
    if answer.refused() {
        return Ok(Answered::Refused(json(&answer).map(|Refusal { error }| error)));
    }
    if !matches!(answer.status, 200 | 201) {
        return Err(answer.failure());
    }
    let Some(Given { client_id, token_endpoint_auth_method, client_secret }) = json(&answer) else {
        return Err(no_client(answer));
    };
    // One that names no method has the default, `client_secret_basic`, by RFC 7591, section 2, which a secret shows it
    // was registered with.
    let with_secret = token_endpoint_auth_method.map_or(client_secret.is_some(), |method| method != NO_SECRET);
    if with_secret {
        return Ok(Answered::WithSecret);
    }
    ClientId::given(&client_id).map(Answered::Registered).ok_or_else(|| no_client(answer))
}

fn no_client(answer: Answer) -> Failure {
    Failure::Unreadable { host: answer.host, what: "no client ID that could be one" }
}
