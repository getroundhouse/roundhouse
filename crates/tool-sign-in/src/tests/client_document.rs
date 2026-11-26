use std::path::Path;

use protocol::client::tool_sign_ins::{REDIRECT_PATH, REDIRECT_PORTS, redirect};
use serde_json::Value;

use crate::registering::client_metadata;
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
    let redirects: Vec<_> = redirects.iter().map(|redirect| RedirectUri::parse(redirect).unwrap()).collect();
    let mut expected = client_metadata(&redirects);
    expected["client_id"] = CLIENT_DOCUMENT.into();
    assert_eq!(document, expected);
}
