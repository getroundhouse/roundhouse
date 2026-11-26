//! What a server says of signing in as it refuses a request, in its `WWW-Authenticate`.

use std::mem;

use crate::model::ToolScopes;
use http::HeaderMap;
use http::header::WWW_AUTHENTICATE;

/// What RFC 6750, section 3.1, calls a token that's good, and isn't allowed what was asked.
const INSUFFICIENT_SCOPE: &str = "insufficient_scope";

/// What a server's 403, with these `headers`, says its access token would have to allow as well, by MCP's "Scope
/// Challenge Handling". Empty where it says that's why it refused, and not what it wants. `None` where it doesn't say
/// that's why. It's the server's own text, of any number and length.
pub fn access_wanted(headers: &HeaderMap) -> Option<ToolScopes> {
    let Challenge { error, scope, .. } = Challenge::of(headers);
    (error? == INSUFFICIENT_SCOPE).then(|| scope.unwrap_or_default())
}

/// What a refusal says of signing in, as RFC 9728, section 5.1, and RFC 6750, section 3, have it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Challenge {
    /// Where the server's metadata is. Any scheme's may say it.
    pub resource_metadata: Option<String>,
    /// What to ask to be allowed, which `Bearer`'s says.
    pub scope: Option<ToolScopes>,
    /// Why it refused, which `Bearer`'s says.
    pub error: Option<String>,
}

impl Challenge {
    pub fn of(headers: &HeaderMap) -> Self {
        let mut challenge = Challenge::default();
        for value in headers.get_all(WWW_AUTHENTICATE) {
            let mut scheme = String::new();
            for part in parts(&String::from_utf8_lossy(value.as_bytes())) {
                let (starts, parameter) = split_scheme(&part);
                if let Some(starts) = starts {
                    scheme = starts.to_ascii_lowercase();
                }
                challenge.take(&scheme, parameter);
            }
        }
        challenge
    }

    /// Takes `parameter`, such as `scope="read write"`, of `scheme`, unless it's said already.
    fn take(&mut self, scheme: &str, parameter: &str) {
        let Some((name, value)) = parameter.split_once('=') else { return };
        let value = unquoted(value.trim());
        match name.trim().to_ascii_lowercase().as_str() {
            "resource_metadata" => {
                self.resource_metadata.get_or_insert(value);
            }
            "scope" if scheme == "bearer" => {
                self.scope.get_or_insert_with(|| ToolScopes::from(value));
            }
            "error" if scheme == "bearer" => {
                self.error.get_or_insert(value);
            }
            _ => {}
        }
    }
}

/// What's between the commas of `header`, but for a comma in quotes. Each is a scheme, a parameter, or a scheme and its
/// first parameter.
fn parts(header: &str) -> Vec<String> {
    let (mut parts, mut part) = (Vec::new(), String::new());
    let (mut quoted, mut escaped) = (false, false);
    for c in header.chars() {
        if c == ',' && !quoted {
            parts.push(mem::take(&mut part).trim().to_string());
            continue;
        }
        if c == '"' && !escaped {
            quoted = !quoted;
        }
        escaped = quoted && c == '\\' && !escaped;
        part.push(c);
    }
    parts.push(part.trim().to_string());
    parts
}

/// The scheme `part` starts with, if it does, and the rest. A parameter's name has `=` after it, with spaces or not,
/// and a scheme has a space, or nothing.
fn split_scheme(part: &str) -> (Option<&str>, &str) {
    let (first, rest) = part.split_once(' ').unwrap_or((part, ""));
    if first.contains('=') || rest.trim_start().starts_with('=') { (None, part) } else { (Some(first), rest) }
}

/// `value`, without its quotes and what escapes a character in them.
fn unquoted(value: &str) -> String {
    let Some(inside) = value.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) else {
        return value.to_string();
    };
    let mut unquoted = String::new();
    let mut chars = inside.chars();
    while let Some(c) = chars.next() {
        unquoted.extend(if c == '\\' { chars.next() } else { Some(c) });
    }
    unquoted
}
