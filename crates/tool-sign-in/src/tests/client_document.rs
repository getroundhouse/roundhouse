use std::path::Path;

use protocol::client::tool_sign_ins::{REDIRECT_PATH, REDIRECT_PORTS, redirect};
use serde_json::{Value, json};

use crate::{CLIENT_DOCUMENT, RedirectUri};

/// The site publishes what's in `site/` at the address it's at there. An authorization server refuses a sign-in whose
/// redirect the document doesn't list, or whose client's address isn't the document's own.
#[test]
fn the_client_document_the_site_publishes_is_roundhouses_and_lists_each_redirect_a_client_listens_at() {
    let path = CLIENT_DOCUMENT.strip_prefix("https://getroundhouse.dev/").unwrap();
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site").join(path);

    let document: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();

    let mut redirects = vec![format!("http://127.0.0.1{REDIRECT_PATH}")];
    redirects.extend(REDIRECT_PORTS.map(redirect));
    let expected = json!({
        "client_id": CLIENT_DOCUMENT,
        "client_name": "Roundhouse",
        "client_uri": "https://getroundhouse.dev",
        "application_type": "native",
        "redirect_uris": redirects,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    assert_eq!(document, expected);
    assert!(redirects.iter().all(|redirect| RedirectUri::parse(redirect).is_ok()));
}
