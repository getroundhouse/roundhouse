//! The link a phone opens to pair with a hub, from a QR code its camera reads.
//!
//! `roundhouse://pair?v=1&name=…&fp=…&code=…&host=…&host=…`, with one `host` for each address the phone might reach
//! the hub at, in the order to try them. `v` is the link's own version, apart from the protocol's: a link with a
//! higher one is refused, so an older app says to update rather than misread it, and parameters this build doesn't
//! know are ignored, so a newer hub can add some without a new version.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Fingerprint, PairingCode};

/// The version of the link's format this build writes, and the newest it reads. Raising it means raising
/// [`crate::PROTOCOL_VERSION`] too, since a hub's reply to `pairing_start` carries a link, and a client that can't read
/// the link can't read the reply.
pub const LINK_VERSION: u32 = 1;

const PREFIX: &str = "roundhouse://pair?";

/// Everything a phone needs to pair with a hub. A record: [`PairingLink::parse`] checks a link it's given, and a
/// hub fills one in from what it knows. On the wire, it's the link's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingLink {
    /// What the hub's machine is called, for people.
    pub hub_name: String,
    /// Each `host:port` the hub might be reached at, in the order to try them.
    pub hosts: Vec<String>,
    /// A connection to anything else is refused, whatever its certificate says.
    pub fingerprint: Fingerprint,
    pub code: PairingCode,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("this isn't a Roundhouse pairing link")]
    NotALink,
    #[error("this pairing link is from a newer version of Roundhouse, with link version {0}. Update to use it")]
    Newer(u32),
    #[error("this pairing link has no {0}")]
    Missing(&'static str),
    #[error("this pairing link's {0} isn't valid")]
    Invalid(&'static str),
}

impl PairingLink {
    pub fn parse(link: &str) -> Result<PairingLink, LinkError> {
        let prefix = link.get(..PREFIX.len()).filter(|prefix| prefix.eq_ignore_ascii_case(PREFIX));
        let query = prefix.map(|_| &link[PREFIX.len()..]).ok_or(LinkError::NotALink)?;
        let query = query.split_once('#').map_or(query, |(query, _)| query);
        let (mut version, mut hub_name, mut fingerprint, mut code, mut hosts) = (None, None, None, None, Vec::new());
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = || decode(value).ok_or(LinkError::Invalid(field_name(key)));
            match key {
                "v" => version = version.or(Some(value()?)),
                "name" => hub_name = hub_name.or(Some(value()?)),
                "fp" => fingerprint = fingerprint.or(Some(value()?)),
                "code" => code = code.or(Some(value()?)),
                "host" => hosts.push(value()?),
                _ => {}
            }
        }
        let version = version.ok_or(LinkError::Missing("version"))?;
        match version.parse::<u32>() {
            Ok(LINK_VERSION) => {}
            Ok(newer) if newer > LINK_VERSION => return Err(LinkError::Newer(newer)),
            Ok(_) | Err(_) => return Err(LinkError::Invalid("version")),
        }
        let hub_name = hub_name.filter(|name| !name.is_empty()).ok_or(LinkError::Missing("hub name"))?;
        let fingerprint = fingerprint.ok_or(LinkError::Missing("fingerprint"))?;
        let fingerprint = Fingerprint::try_from(fingerprint).map_err(|_| LinkError::Invalid("fingerprint"))?;
        let code = PairingCode::from(code.filter(|code| !code.is_empty()).ok_or(LinkError::Missing("code"))?);
        if hosts.is_empty() {
            return Err(LinkError::Missing("host"));
        }
        if !hosts.iter().all(|host| is_host_and_port(host)) {
            return Err(LinkError::Invalid("host"));
        }
        Ok(PairingLink { hub_name, hosts, fingerprint, code })
    }
}

impl fmt::Display for PairingLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}v={LINK_VERSION}&name={}", Encoded(&self.hub_name))?;
        write!(f, "&fp={}&code={}", Encoded(self.fingerprint.as_str()), Encoded(self.code.as_str()))?;
        for host in &self.hosts {
            write!(f, "&host={}", Encoded(host))?;
        }
        Ok(())
    }
}

impl Serialize for PairingLink {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PairingLink {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PairingLink::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn field_name(key: &str) -> &'static str {
    match key {
        "v" => "version",
        "name" => "hub name",
        "fp" => "fingerprint",
        "code" => "code",
        _ => "host",
    }
}

/// A name or address, then a port that isn't 0. An IPv6 address would be in brackets, so its colons come first.
fn is_host_and_port(text: &str) -> bool {
    text.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok_and(|port| port > 0)
    })
}

/// Leaves only what a URL never needs escaped, plus the colon in `host:port`. The rest is percent-encoded, including
/// `+`, which some readers take as a space.
struct Encoded<'a>(&'a str);

impl fmt::Display for Encoded<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b':') {
                write!(f, "{}", char::from(byte))?;
            } else {
                write!(f, "%{byte:02X}")?;
            }
        }
        Ok(())
    }
}

/// Undoes percent-encoding. `+` stays a `+`. `None` if an escape is broken or the result isn't UTF-8.
fn decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        if byte == b'%' {
            let hex = std::str::from_utf8(after.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &after[2..];
        } else {
            bytes.push(byte);
            rest = after;
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINGERPRINT: &str = "q3B0ZWxsZXJzLWZpbmdlcnByaW50LXNoYTI1Ni1oYXN";

    fn link() -> PairingLink {
        PairingLink {
            hub_name: "Ada’s MacBook Pro".to_string(),
            hosts: vec!["Adas-MacBook-Pro.local:7867".to_string(), "100.64.0.7:7867".to_string()],
            fingerprint: FINGERPRINT.parse().unwrap(),
            code: PairingCode::from("c0de"),
        }
    }

    #[test]
    fn a_link_reads_as_a_url_with_its_hosts_in_order() {
        assert_eq!(
            link().to_string(),
            format!(
                "roundhouse://pair?v=1&name=Ada%E2%80%99s%20MacBook%20Pro&fp={FINGERPRINT}&code=c0de&host=Adas-MacBook-Pro.local:7867&host=100.64.0.7:7867"
            )
        );
    }

    #[test]
    fn parameters_this_build_does_not_know_are_ignored() {
        let newer = format!("{}&relay=example&host=10.0.0.2:7867#later", link());

        let parsed = PairingLink::parse(&newer).unwrap();

        assert_eq!(parsed.hosts.last().map(String::as_str), Some("10.0.0.2:7867"));
    }

    #[test]
    fn a_link_from_a_newer_format_says_so() {
        let newer = link().to_string().replace("v=1", "v=2");

        assert_eq!(PairingLink::parse(&newer), Err(LinkError::Newer(2)));
    }

    #[test]
    fn what_a_link_needs_is_checked() {
        let without = |part: &str| link().to_string().replace(part, "");

        assert_eq!(PairingLink::parse("https://example.com/pair?v=1"), Err(LinkError::NotALink));
        assert_eq!(PairingLink::parse(&without("v=1&")), Err(LinkError::Missing("version")));
        assert_eq!(PairingLink::parse(&without("&code=c0de")), Err(LinkError::Missing("code")));
        let no_hosts = link().to_string().split("&host").next().unwrap().to_string();
        assert_eq!(PairingLink::parse(&no_hosts), Err(LinkError::Missing("host")));
        let short = link().to_string().replace(FINGERPRINT, "abc");
        assert_eq!(PairingLink::parse(&short), Err(LinkError::Invalid("fingerprint")));
        let no_port = link().to_string().replace("100.64.0.7:7867", "100.64.0.7");
        assert_eq!(PairingLink::parse(&no_port), Err(LinkError::Invalid("host")));
        let port_zero = link().to_string().replace("100.64.0.7:7867", "100.64.0.7:0");
        assert_eq!(PairingLink::parse(&port_zero), Err(LinkError::Invalid("host")));
        let broken = link().to_string().replace("%E2%80%99", "%E2%8");
        assert_eq!(PairingLink::parse(&broken), Err(LinkError::Invalid("hub name")));
    }
}
