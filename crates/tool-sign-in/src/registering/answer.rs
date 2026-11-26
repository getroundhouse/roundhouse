//! What an authorization server answers a client that asks to be registered, as RFC 7591, section 3.2, has it.

use serde::Deserialize;

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

/// What's read of a registration. A secret in it, and a token to change it with, are left unread.
#[derive(Deserialize)]
struct Given {
    client_id: String,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
}

pub(super) fn answered(answer: Answer) -> Result<Answered, Failure> {
    if answer.refused() {
        return Ok(Answered::Refused(json(&answer).map(|Refusal { error }| error)));
    }
    if !matches!(answer.status, 200 | 201) {
        return Err(answer.failure());
    }
    let Some(Given { client_id, token_endpoint_auth_method }) = json(&answer) else {
        return Err(no_client(answer));
    };
    if token_endpoint_auth_method.is_some_and(|method| method != NO_SECRET) {
        return Ok(Answered::WithSecret);
    }
    ClientId::given(&client_id).map(Answered::Registered).ok_or_else(|| no_client(answer))
}

fn no_client(answer: Answer) -> Failure {
    Failure::Unreadable { host: answer.host, what: "no client ID that could be one" }
}
