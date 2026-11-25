//! Why a server failed, from what Codex says of it, without the secrets that can be in that.

/// How much of it is kept, which is a line of a card.
const LONGEST: usize = 200;

/// Where what went wrong starts and ends in Codex's account of a connection, which has its own Rust types before and
/// then says the same again after.
const ACCOUNT_STARTS: &str = "] error: ";
const ACCOUNT_ENDS: &str = ", when ";

/// What Codex puts before what went wrong where it has no such account.
const STARTUP_FAILED: &str = "MCP startup failed: ";

/// What Codex puts before the status of a reply it can't use, which the reply's body follows.
const REPLIED: &str = "server response: HTTP ";

/// The first line of `text`, or the part of it that says what went wrong, on one line. Codex gives a server's URL
/// whole, with the secrets its path and its query can hold, and the body of a reply, so it keeps only a URL's origin,
/// and of a reply only its status. What follows `Bearer` or `Basic` goes too.
pub(super) fn why(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let said = match line.split_once(ACCOUNT_STARTS) {
        Some((_, account)) => account.split(ACCOUNT_ENDS).next().unwrap_or(account),
        None => line.split_once(STARTUP_FAILED).map_or(line, |(_, after)| after),
    };
    let said = until_status(said);
    let mut words = Vec::new();
    let mut after_scheme = false;
    for word in said.split_whitespace() {
        words.push(if after_scheme { "…".to_owned() } else { with_origin(word) });
        let bare = word.trim_matches(|c: char| !c.is_alphanumeric());
        after_scheme = bare.eq_ignore_ascii_case("bearer") || bare.eq_ignore_ascii_case("basic");
    }
    let said = words.join(" ");
    let kept: String = said.chars().take(LONGEST).collect();
    Some(if kept.len() < said.len() { format!("{kept}…") } else { kept }).filter(|said| !said.is_empty())
}

/// `said` as far as the status of a reply, if it tells of one.
fn until_status(said: &str) -> &str {
    let Some(at) = said.find(REPLIED) else { return said };
    let status = at + REPLIED.len();
    let digits = said[status..].bytes().take_while(u8::is_ascii_digit).count();
    &said[..status + digits]
}

/// `word`, with a URL in it cut to its scheme, host and port.
fn with_origin(word: &str) -> String {
    let Some((before, after)) = word.split_once("://") else { return word.to_owned() };
    let authority = after.split(['/', '?', '#', ')', '"', '\'', '`', '>', ',']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let close = if before.starts_with('(') { ")" } else { "" };
    format!("{before}://{host}{close}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As 0.171.0 says one it can't reach, twice over.
    fn account(error: &str) -> String {
        let one = format!("Send message error Transport [client::ToolListChanges<client::Adapter>>>>] error: {error}");
        format!(
            "MCP client for `docs` failed to start: MCP startup failed: handshaking with MCP server failed: \
             {one}, when send initialize request: {one}, when send initialize request"
        )
    }

    #[test]
    fn what_went_wrong_is_kept_without_the_secrets_around_it() {
        let cases = [
            (
                account(
                    "Client error: error sending request for url (https://ada:pw@mcp.example.com:8443/s/secret?key=secret)",
                ),
                "Client error: error sending request for url (https://mcp.example.com:8443)",
            ),
            (
                account("unexpected server response: HTTP 500: secret in the reply, and more"),
                "unexpected server response: HTTP 500",
            ),
            (
                "MCP client for `docs` failed to start: MCP startup failed: No such file or directory (os error 2)"
                    .into(),
                "No such file or directory (os error 2)",
            ),
            (
                "MCP client for `docs` timed out after 6 seconds. Add or adjust it:\n[mcp_servers.docs]\nkey = secret"
                    .into(),
                "MCP client for `docs` timed out after 6 seconds. Add or adjust it:",
            ),
            ("refused Authorization: Bearer secret.token here".into(), "refused Authorization: Bearer … here"),
            ("see\thttps://mcp.example.com/secret#secret".into(), "see https://mcp.example.com"),
        ];

        for (text, kept) in cases {
            assert_eq!(why(&text).as_deref(), Some(kept), "{text}");
        }
        assert_eq!(why(" \n "), None);
    }

    #[test]
    fn a_long_one_is_cut_between_characters() {
        let kept = why(&"é".repeat(LONGEST + 1)).unwrap();

        assert_eq!(kept, format!("{}…", "é".repeat(LONGEST)));
    }
}
