use protocol::worker::PushRoute;

/// Where a push to `addresses` goes, one to a line, as `git remote get-url --push --all` gives each address a push
/// goes to: through the git credential only if every one does, since git pushes to each, and round it if there's none.
pub(super) fn route(addresses: &str) -> PushRoute {
    let mut addresses = addresses.lines().map(str::trim).filter(|address| !address.is_empty()).peekable();
    if addresses.peek().is_some() && addresses.all(through_the_credential) {
        PushRoute::Credential
    } else {
        PushRoute::Around
    }
}

/// Whether git pushes to `address` with the worker's credential helper, which its git config names for
/// `https://github.com` alone: an `https://` address on github.com, with no port, which git takes for another, and no
/// password, which git would push with instead. A user alone is still the helper's to answer for. *Unconfirmed:* an
/// address with a port, or with a password, counting as one that goes round the credential.
fn through_the_credential(address: &str) -> bool {
    let scheme = "https://";
    let Some(prefix) = address.get(..scheme.len()).filter(|it| it.eq_ignore_ascii_case(scheme)) else { return false };
    let Some((authority, _)) = address[prefix.len()..].split_once('/') else { return false };
    let host = match authority.rsplit_once('@') {
        Some((user, _)) if user.contains(':') => return false,
        Some((_, host)) => host,
        None => authority,
    };
    host.eq_ignore_ascii_case("github.com")
}
