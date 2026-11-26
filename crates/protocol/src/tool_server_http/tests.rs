use super::{Challenge, access_wanted, is_private};
use crate::model::ToolScopes;
use http::HeaderMap;
use http::header::WWW_AUTHENTICATE;
use std::net::IpAddr;

fn scopes(text: &str) -> ToolScopes {
    ToolScopes::from(text.to_string())
}

#[test]
fn a_401_says_where_to_look_and_what_to_ask_for_among_whatever_else_it_says() {
    let place = "https://tools.example/.well-known/oauth-protected-resource";
    let said =
        |scope: &str| Challenge { resource_metadata: Some(place.to_string()), scope: Some(scopes(scope)), error: None };
    let cases = [
        (vec![format!(r#"Bearer resource_metadata="{place}", scope="read write""#)], said("read write")),
        (
            vec![format!(r#"bearer error="invalid_token",scope=read ,Resource_Metadata = "{place}""#)],
            Challenge { error: Some("invalid_token".to_string()), ..said("read") },
        ),
        (
            vec![format!(
                r#"Basic realm="a, \"b\", c", Bearer realm="d, scope=other", scope="read", resource_metadata="{place}""#
            )],
            said("read"),
        ),
        (vec!["Negotiate".into(), format!(r#"Bearer scope="read", resource_metadata="{place}""#)], said("read")),
        (vec![format!(r#"Bearer scope="read", DPoP resource_metadata="{place}", scope="other""#)], said("read")),
        (vec![r#"Digest scope="other", Basic"#.into(), "Bearer".into()], Challenge::default()),
        (vec![], Challenge::default()),
    ];
    for (values, expected) in cases {
        let mut headers = HeaderMap::new();
        for value in &values {
            headers.append(WWW_AUTHENTICATE, value.parse().unwrap());
        }
        assert_eq!(Challenge::of(&headers), expected, "{values:?}");
    }
}

#[test]
fn a_403_says_which_access_it_wants_only_where_its_bearer_challenge_says_thats_why() {
    let wants = r#"Bearer error="insufficient_scope", scope="read \"x\" write""#;
    let cases = [
        (vec![wants], Some(scopes("read write"))),
        (vec!["Basic realm=\"a\"", "bearer scope=read, Error=insufficient_scope"], Some(scopes("read"))),
        (vec![r#"Bearer error="insufficient_scope", error_description="made, up""#], Some(scopes(""))),
        (vec![r#"Bearer error="insufficient_scope", scope="""#], Some(scopes(""))),
        (vec![r#"Bearer error="invalid_token", scope="read""#], None),
        (vec![r#"DPoP error="insufficient_scope", scope="read""#], None),
        (vec![r#"Bearer scope="read""#], None),
        (vec![], None),
    ];
    for (values, expected) in cases {
        let mut headers = HeaderMap::new();
        for value in &values {
            headers.append(WWW_AUTHENTICATE, value.parse().unwrap());
        }
        assert_eq!(access_wanted(&headers), expected, "{values:?}");
    }
}

/// This machine's, a private network's, the link's, and what's kept for something else, with IPv4's written as IPv6's.
const PRIVATE: &str = "127.0.0.1 127.9.9.9 0.0.0.0 ::1 :: 10.1.2.3 172.16.0.1 172.31.255.255 192.168.1.1 \
    169.254.169.254 100.64.0.1 100.127.255.255 fd12::1 fc00::1 fe80::1 fec0::1 192.0.0.8 192.0.2.1 192.88.99.1 \
    198.18.0.1 198.19.255.255 198.51.100.1 203.0.113.1 224.0.0.1 240.0.0.1 255.255.255.255 ff02::1 2001::1 \
    2001:1ff::1 2001:db8::1 3fff::1 ::ffff:127.0.0.1 ::ffff:10.0.0.1 ::127.0.0.1 64:ff9b::a9fe:a9fe 64:ff9b:1::1 \
    2002:7f00:1::1 2002:c0a8:101::1";

/// Either side of those, and IPv4's on the internet written as IPv6's.
const ON_THE_INTERNET: &str = "1.1.1.1 11.0.0.1 100.63.255.255 100.128.0.1 172.15.0.1 172.32.0.1 192.0.1.1 \
    192.167.1.1 198.17.0.1 198.20.0.1 223.255.255.255 2606:4700::1111 2001:200::1 2001:4860::8888 3ffe::1 \
    ::ffff:1.1.1.1 64:ff9b::101:101 2002:101:101::1";

#[test]
fn an_address_is_one_machines_on_the_internet_or_private() {
    for (these, expected) in [(PRIVATE, true), (ON_THE_INTERNET, false)] {
        for address in these.split_whitespace() {
            assert_eq!(is_private(address.parse::<IpAddr>().unwrap()), expected, "{address}");
        }
    }
}
