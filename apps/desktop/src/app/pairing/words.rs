//! Parsing a typed pairing link for instant preview, and explaining why pairing couldn't finish.

use client_core::ClientError;
use protocol::pairing::{LinkError, PairingLink};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ParsedInput {
    Empty,
    Valid(PairingLink),
    Invalid(String),
}

pub(super) fn parse_input(raw: &str) -> ParsedInput {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ParsedInput::Empty;
    }
    match PairingLink::parse(trimmed) {
        Ok(link) => ParsedInput::Valid(link),
        Err(LinkError::Newer(version)) => ParsedInput::Invalid(format!(
            "This pairing link is from a newer version of Roundhouse (link version {version}). Update this app to use \
             it."
        )),
        Err(LinkError::NotALink) => ParsedInput::Invalid("Paste a roundhouse://pair?… link.".to_string()),
        Err(LinkError::Missing(field) | LinkError::Invalid(field)) => {
            ParsedInput::Invalid(format!("This pairing link’s {field} is missing or incomplete."))
        }
    }
}

/// Returns `(message, select_field_for_replacement)` for an unsuccessful `client_core::pair` attempt.
pub(super) fn connect_failure(link: &PairingLink, error: &ClientError) -> (String, bool) {
    let hub = &link.hub_name;
    match error {
        ClientError::Unreachable { .. } | ClientError::Io(_) | ClientError::Disconnected => (
            format!(
                "Couldn’t reach {hub} at {}. Check that it’s awake and on the same network or VPN, then try again.",
                link.hosts.join(" or ")
            ),
            false,
        ),
        ClientError::FingerprintMismatch { host } => (
            format!(
                "{host} answered with a different key than this link’s. Check that you’re reaching {hub}, or create \
                 a new link there."
            ),
            true,
        ),
        ClientError::Api(_) if error.is_unauthorized() => (
            format!("That pairing link has expired or was already used. Create a new link on {hub} and paste it here."),
            true,
        ),
        ClientError::Api(api) if error.is_version_mismatch() => (
            format!("{hub} is on another protocol version ({}). Update Roundhouse on the older machine.", api.message),
            false,
        ),
        ClientError::Api(api) => (format!("Couldn’t connect to {hub}: {}", api.message), false),
        ClientError::Unexpected(detail) => (format!("Couldn’t connect to {hub}: {detail}"), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_distinguishes_empty_invalid_and_valid_links() {
        assert_eq!(parse_input("   "), ParsedInput::Empty);
        assert!(matches!(parse_input("https://example.com"), ParsedInput::Invalid(_)));
        let valid = "roundhouse://pair?v=1&name=Mac%20Studio&fp=q3B0ZWxsZXJzLWZpbmdlcnByaW50LXNoYTI1Ni1oYXN&code=c0de&host=100.64.0.7:7867";
        let ParsedInput::Valid(parsed) = parse_input(valid) else { panic!("expected valid link") };
        assert_eq!(parsed.hub_name, "Mac Studio");
    }

    #[test]
    fn unreachable_errors_keep_the_link_while_unauthorized_asks_for_a_new_one() {
        let valid = "roundhouse://pair?v=1&name=Mac%20Studio&fp=q3B0ZWxsZXJzLWZpbmdlcnByaW50LXNoYTI1Ni1oYXN&code=c0de&host=100.64.0.7:7867";
        let link = PairingLink::parse(valid).unwrap();
        let unreachable =
            ClientError::Unreachable { hosts: vec!["100.64.0.7:7867".into()], last: None, not_pinned: Vec::new() };
        let (msg, replace) = connect_failure(&link, &unreachable);
        assert!(!replace);
        assert!(msg.contains("Check that it’s awake"), "{msg}");

        let used =
            ClientError::Api(protocol::client::ApiError::new(protocol::client::ErrorCode::Unauthorized, "expired"));
        let (msg, replace) = connect_failure(&link, &used);
        assert!(replace);
        assert!(msg.contains("expired or was already used"), "{msg}");
    }
}
