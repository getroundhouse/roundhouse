//! Where metadata is looked for, in MCP's order.

use url::Url;

/// Where the metadata of the server at `server` is, where its 401 doesn't say: for its path, and then for its host, as
/// RFC 9728, section 3.1, makes the address.
pub(super) fn of_server(server: &Url) -> Vec<Url> {
    let suffix = "oauth-protected-resource";
    let root = inserted(&host_of(server), suffix);
    let own = inserted(server, suffix);
    if own == root { vec![root] } else { vec![own, root] }
}

/// Where the metadata of the authorization server at `issuer` is: OAuth's, as RFC 8414, section 3.1, makes the address,
/// and OIDC's, made the same way and by its own.
pub(super) fn of_authorization_server(issuer: &Url) -> Vec<Url> {
    let openid = "openid-configuration";
    let mut addresses = vec![inserted(issuer, "oauth-authorization-server"), inserted(issuer, openid)];
    let path = issuer.path().trim_end_matches('/');
    if !path.is_empty() {
        let mut appended = issuer.clone();
        appended.set_path(&format!("{path}/.well-known/{openid}"));
        addresses.push(appended);
    }
    addresses
}

/// `url`, with no path.
pub(super) fn host_of(url: &Url) -> Url {
    let mut root = url.clone();
    root.set_path("");
    root.set_query(None);
    root
}

/// `url`, with the well-known path `suffix` between its host and its own path, which a `/` at its end isn't part of.
fn inserted(url: &Url, suffix: &str) -> Url {
    let mut inserted = url.clone();
    inserted.set_path(&format!("/.well-known/{suffix}{}", url.path().trim_end_matches('/')));
    inserted
}
