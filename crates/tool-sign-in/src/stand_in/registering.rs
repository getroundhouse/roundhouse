use serde_json::{Value, json};

use super::Asked;
use super::replies::{Reply, said};
use super::script::{Registers, Script};
use super::tokens::Tokens;

/// What `POST /register` answers `asked`. Panics where that isn't what RFC 7591 has a client with no secret ask.
pub(super) fn answer(asked: &Asked, script: &Script, tokens: &mut Tokens) -> Reply {
    let client: Value = serde_json::from_str(&asked.body).unwrap();
    assert_eq!(asked.headers["content-type"], "application/json");
    assert_eq!(client["token_endpoint_auth_method"], "none");
    let redirects: Vec<String> = serde_json::from_value(client["redirect_uris"].clone()).unwrap();
    assert!(!redirects.is_empty(), "somewhere to send the person back to");
    match &script.registers {
        Registers::Anyone => {
            let mut given = client;
            given["client_id"] = tokens.register(redirects).into();
            given["client_secret"] = "made-up-secret".into();
            given["registration_access_token"] = "made-up-registration-token".into();
            said(201, &given)
        }
        Registers::Says(status, body) => said(*status, body),
        Registers::Nobody | Registers::At(_) => said(404, &json!({})),
    }
}
