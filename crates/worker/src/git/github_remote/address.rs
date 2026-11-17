use protocol::model::GithubRepositoryName;

/// The schemes git reaches a host by, in an address that names one.
const SCHEMES: [&str; 6] = ["https://", "http://", "ssh://", "git://", "git+ssh://", "ssh+git://"];

/// The repository on github.com that the address `address` names, as git takes an address: a URL, such as
/// `https://github.com/cli/cli.git` or `ssh://git@github.com/cli/cli`, or one in scp's form, such as
/// `git@github.com:cli/cli.git`, with or without `.git` and a `/` at its end. `None` for any other host, a path, or what
/// isn't an owner and a name.
pub(super) fn on_github(address: &str) -> Option<GithubRepositoryName> {
    let (host, path) = host_and_path(address)?;
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }
    let path = path.trim_start_matches('/').trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    GithubRepositoryName::new(owner, name).ok()
}

/// The host `address` names, and the path on it, if it names a host.
fn host_and_path(address: &str) -> Option<(&str, &str)> {
    if let Some(rest) = SCHEMES.iter().find_map(|scheme| strip_prefix_ignoring_case(address, scheme)) {
        let (authority, path) = rest.split_once('/')?;
        // After any user and password, and before any port.
        let host = authority.rsplit('@').next()?;
        return Some((host.split(':').next()?, path));
    }
    // scp's form, which git takes only with no `/` before its first `:`, so a path with a colon in it isn't.
    let (before, path) = address.split_once(':')?;
    if before.contains('/') {
        return None;
    }
    Some((before.rsplit('@').next()?, path))
}

/// What's after `prefix` in `text`, if `text` starts with it, whatever its case, as git reads a scheme.
fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let starts = text.get(..prefix.len())?;
    starts.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}
