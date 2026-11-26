use std::mem;

use http::HeaderMap;
use http::header::WWW_AUTHENTICATE;

use crate::Scopes;

/// What a 401 says of signing in, in its `WWW-Authenticate`, as RFC 9728, section 5.1, and RFC 6750, section 3, have
/// it.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Challenge {
    /// Where the server's metadata is. Any scheme's may say it.
    pub(super) resource_metadata: Option<String>,
    /// What to ask to be allowed, which `Bearer`'s says.
    pub(super) scope: Option<Scopes>,
}

impl Challenge {
    pub(super) fn of(headers: &HeaderMap) -> Self {
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
                self.scope.get_or_insert_with(|| Scopes::from(value));
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
