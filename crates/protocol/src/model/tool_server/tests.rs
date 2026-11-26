use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

fn named(text: &str) -> Result<String, ToolServerNameError> {
    ToolServerName::typed(text).map(|name| name.as_str().to_owned())
}

fn for_agents(text: &str) -> String {
    ToolServerName::stored(text.to_owned()).for_agents().as_str().to_owned()
}

#[test]
fn a_name_is_whatever_a_person_types_trimmed_but_for_none_a_long_one_one_with_control_characters_and_the_workers() {
    for name in ["Linear (Northwind)", "Café.docs", "日本語", "a__b", "roundhouse 2"] {
        assert_eq!(named(&format!(" {name}\n")), Ok(name.to_owned()));
    }
    assert_eq!(named(&"é".repeat(64)), Ok("é".repeat(64)), "counted in characters");
    assert_eq!(named(&"x".repeat(65)), Err(ToolServerNameError::TooLong { most: 64 }));
    assert_eq!(named(" \n "), Err(ToolServerNameError::Empty));
    for broken in ["two\nlines", "a\ttab", "a\u{7f}delete", "next\u{85}line"] {
        assert_eq!(named(broken), Err(ToolServerNameError::ControlCharacter), "{broken:?}");
    }
    for own in ["roundhouse", "Roundhouse", "(Roundhouse)"] {
        assert_eq!(named(own), Err(ToolServerNameError::RoundhousesOwn), "{own}");
    }
}

#[test]
fn agents_call_a_server_by_what_every_cli_takes_of_its_name() {
    let made = [
        ("Linear", "Linear"),
        ("Linear (Northwind)", "Linear_Northwind"),
        ("Linear  Northwind", "Linear_Northwind"),
        ("linear_dash-1", "linear_dash-1"),
        ("Café.docs", "Caf_docs"),
        ("a__b", "a_b"),
        ("-_a_-", "a"),
        ("日本語", "server"),
        ("--", "server"),
        ("twenty eight characters long and more", "twenty_eight_characters_long_and"),
        // Cut at what's between two words, which isn't left at the end.
        ("abcdefghijklmnopqrstuvwxyz12345 rest", "abcdefghijklmnopqrstuvwxyz12345"),
    ];

    for (name, agents) in made {
        assert_eq!(for_agents(name), agents, "{name}");
    }
}

#[test]
fn two_names_are_the_same_for_agents_when_only_case_or_what_they_cant_take_differs() {
    let same =
        |a: &str, b: &str| ToolServerName::typed(a).unwrap().is_same_for_agents(&ToolServerName::typed(b).unwrap());

    assert!(same("Linear", "LINEAR"));
    assert!(same("Linear (Northwind)", "linear northwind"));
    assert!(same("日本語", "Server"));
    assert!(!same("Linear", "Linear 2"));
    assert!(!same("Linear-2", "Linear 2"));
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Letters, digits, `-` and `_`, with none of the last two at an end, and no two `_` together.
    #[test]
    fn what_agents_call_a_server_is_one_every_cli_takes_and_agents_call_that_the_same(
        name in prop_oneof![any::<String>(), "[a-zA-Z0-9 _.()é-]{0,80}", "[ _-]{0,4}[a-z]{28,34}[ _-]{0,4}[a-z]?"],
    ) {
        let made = for_agents(&name);

        prop_assert!((1..=32).contains(&made.len()), "{made}");
        prop_assert!(made.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{made}");
        prop_assert!(made.starts_with(|c: char| c.is_ascii_alphanumeric()), "{made}");
        prop_assert!(made.ends_with(|c: char| c.is_ascii_alphanumeric()), "{made}");
        prop_assert!(!made.contains("__"), "{made}");
        prop_assert_eq!(for_agents(&made), made);
    }

    /// Whatever's before the `@`, and however the rest of the address is wrong.
    #[test]
    fn a_url_that_holds_a_credential_is_refused_however_its_spelled(
        scheme in "(https?|HTTPS|Http)",
        credential in "[a-z:@%!]{0,12}",
        rest in "[a-z.:]{0,12}(/[a-z@#?]{0,8})?",
    ) {
        let url = format!("{scheme}://{credential}@{rest}");
        prop_assert_eq!(ToolServerUrl::typed(&url), Err(ToolServerUrlError::HoldsCredential), "{}", url);
    }
}

#[test]
fn a_url_is_an_http_address_of_a_host_trimmed_and_otherwise_as_typed() {
    use ToolServerUrlError as E;
    let taken = [
        "https://mcp.example.com/mcp",
        "HTTPS://MCP.Example.com",
        "http://localhost:8080/mcp/",
        "http://127.0.0.1:1/",
        "http://[::1]:8080/mcp",
        "https://mcp.example.com/mcp?team=a@b",
        "https://mcp.example.com/@ada/mcp",
    ];
    for url in taken {
        assert_eq!(ToolServerUrl::typed(&format!(" {url}\n")).map(|url| url.as_str().to_owned()), Ok(url.to_owned()));
    }

    let refused = [
        ("", E::Empty),
        ("mcp.example.com/mcp", E::NotHttp),
        ("/mcp", E::NotHttp),
        ("//mcp.example.com/mcp", E::NotHttp),
        ("ftp://mcp.example.com", E::NotHttp),
        ("stdio://mcp", E::NotHttp),
        ("https://ada:made-up@mcp.example.com/mcp", E::HoldsCredential),
        ("https://made-up@mcp.example.com/mcp", E::HoldsCredential),
        ("https://@mcp.example.com", E::HoldsCredential),
        ("https://mcp.example.com/mcp#tools", E::HasFragment),
        ("https://", E::NotAUrl),
        ("https:///mcp", E::NotAUrl),
        ("https://?mcp", E::NotAUrl),
        ("https://:8080/mcp", E::NotAUrl),
        ("https://mcp.example.com:/mcp", E::NotAUrl),
        ("https://mcp.example.com:0/mcp", E::NotAUrl),
        ("https://mcp.example.com:65536/mcp", E::NotAUrl),
        ("https://mcp example.com/mcp", E::NotAUrl),
        ("https://mcp.example.com/m cp", E::NotAUrl),
        ("https://mcp.example.com/m\ncp", E::NotAUrl),
    ];
    for (url, why) in refused {
        assert_eq!(ToolServerUrl::typed(url), Err(why), "{url:?}");
    }
    let long = format!("https://mcp.example.com/{}", "x".repeat(2048));
    assert_eq!(ToolServerUrl::typed(&long), Err(E::TooLong { most: 2048 }));
}

#[test]
fn a_urls_host_is_what_its_between_the_scheme_and_the_path() {
    for (url, host) in [
        ("https://mcp.example.com/mcp", "mcp.example.com"),
        ("HTTPS://mcp.example.com", "mcp.example.com"),
        ("http://localhost:8080?team=a/b", "localhost:8080"),
        ("http://[::1]:8080/mcp", "[::1]:8080"),
        ("mcp.example.com", "mcp.example.com"),
        ("https://", "https://"),
    ] {
        assert_eq!(ToolServerUrl::stored(url.to_owned()).host(), host);
    }
}

#[test]
fn a_keys_header_is_any_but_one_the_connection_to_the_server_sets() {
    use ToolServerHeaderError as E;
    for header in ["Authorization", "X-API-Key", "x_key", "Cookie"] {
        let typed = ToolServerHeader::typed(&format!(" {header} ")).map(|header| header.as_str().to_owned());
        assert_eq!(typed, Ok(header.to_owned()));
    }

    let reserved = [
        "Host",
        "content-length",
        "CONNECTION",
        "Transfer-Encoding",
        "Mcp-Session-Id",
        "MCP-Protocol-Version",
        "Mcp-Method",
        "Mcp-Name",
        "Accept",
        "Content-Type",
        "Proxy-Authorization",
    ];
    for header in reserved {
        assert_eq!(ToolServerHeader::typed(header), Err(E::Reserved), "{header}");
    }
    assert_eq!(ToolServerHeader::typed(" "), Err(E::Empty));
    for header in ["X Key", "X-Key:", "X-\nKey", "Clé"] {
        assert_eq!(ToolServerHeader::typed(header), Err(E::NotAHeader), "{header:?}");
    }
    assert_eq!(ToolServerHeader::typed(&"x".repeat(129)), Err(E::TooLong { most: 128 }));
}

/// As one from a hub whose rule differs, which failing to read would leave a client with none of the hub's records.
#[test]
fn a_record_whose_name_url_and_header_this_build_would_refuse_still_reads() {
    let name = "x".repeat(65);
    let line = format!(
        r#"{{"id":"t","name":"{name}","url":"mcp.example.com","auth":{{"kind":"key","header":"Host"}},"rev":3}}"#
    );

    let read: ToolServer = serde_json::from_str(&line).unwrap();

    assert_eq!((read.name.as_str(), read.url.as_str()), (name.as_str(), "mcp.example.com"));
    assert!(matches!(read.auth, ToolServerAuth::Key { header } if header.as_str() == "Host"));
}

#[test]
fn servers_are_listed_by_name_in_any_case_then_by_id() {
    let server = |id: &str, name: &str| ToolServer {
        name: ToolServerName::stored(name.to_owned()),
        ..crate::fixtures::tool_server(id)
    };
    let mut servers = [server("3", "Sentry"), server("2", "linear"), server("1", "Docs"), server("0", "Sentry")];

    servers.sort_by(ToolServer::by_name);

    assert_eq!(servers.map(|server| server.id.as_str().to_owned()), ["1", "2", "0", "3"]);
}
