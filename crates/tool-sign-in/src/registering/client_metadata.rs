use serde_json::{Value, json};

use super::NO_SECRET;
use crate::RedirectUri;

/// What Roundhouse says of itself as a client that's sent back to `redirects`. Its document at
/// [`CLIENT_DOCUMENT`](crate::CLIENT_DOCUMENT) says this with its own address as well, as a test holds the document to,
/// and registering sends it as it is.
pub(crate) fn client_metadata(redirects: &[RedirectUri]) -> Value {
    let redirects: Vec<_> = redirects.iter().map(RedirectUri::as_str).collect();
    json!({
        "client_name": "Roundhouse",
        "client_uri": "https://getroundhouse.dev",
        "application_type": "native",
        "redirect_uris": redirects,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": NO_SECRET,
    })
}
