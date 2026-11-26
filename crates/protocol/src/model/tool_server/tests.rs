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

/// Each is compared both ways, with the first of its row.
#[test]
fn two_urls_are_of_one_server_when_they_differ_in_their_query_at_most() {
    let of_one = [
        &["https://mcp.example.com/mcp", "https://mcp.example.com/mcp?tools=a", "https://mcp.example.com/mcp?tools=b"]
            [..],
        &["https://mcp.example.com/mcp", "HTTPS://MCP.Example.com/mcp", "https://mcp.example.com:443/mcp"],
        &["http://127.0.0.1", "http://127.0.0.1/", "http://127.0.0.1:80", "http://127.0.0.1?tools=a"],
        &["http://[::1]:8080/mcp", "http://[::1]:8080/mcp?"],
        &["not a URL", "not a URL"],
    ];
    let of_another = [
        &["https://mcp.example.com/mcp", "http://mcp.example.com/mcp", "http://mcp.example.com:443/mcp"][..],
        &["https://mcp.example.com/mcp", "https://eu.mcp.example.com/mcp", "https://mcp.example.com.evil/mcp"],
        &["https://mcp.example.com/mcp", "https://mcp.example.com:8443/mcp", "https://mcp.example.com:80/mcp"],
        &["https://mcp.example.com/mcp", "https://mcp.example.com/mcp/", "https://mcp.example.com/MCP"],
        &["https://mcp.example.com/mcp", "https://mcp.example.com/", "https://mcp.example.com/mcp%2f"],
        &["https://mcp.example.com/mcp", "not a URL", "https://mcp.example.com/?/mcp"],
        &["not a URL", "not a URL?"],
    ];
    let url = |text: &str| ToolServerUrl::stored(text.to_owned());
    let same = |a: &str, b: &str| [url(a).is_same_server(&url(b)), url(b).is_same_server(&url(a))];

    for row in of_one {
        for other in row {
            assert_eq!(same(row[0], other), [true; 2], "{} and {other}", row[0]);
        }
    }
    for row in of_another {
        for other in &row[1..] {
            assert_eq!(same(row[0], other), [false; 2], "{} and {other}", row[0]);
        }
    }
    let place = serde_json::to_string(&url("HTTPS://MCP.Example.com/mcp?tools=a").place()).unwrap();
    assert_eq!(place, r#""https://mcp.example.com:443/mcp""#, "with no query, which can hold a key");
    let queries =
        ["https://mcp.example.com/mcp?tools=a?b", "https://mcp.example.com/mcp?", "https://mcp.example.com/mcp"];
    assert_eq!(
        queries.map(|text| url(text).query().map(str::to_owned)),
        [Some("tools=a?b".into()), Some(String::new()), None]
    );
}

/// Near misses are the same but for one thing.
#[test]
fn a_url_looks_keyed_by_an_options_name_and_a_long_value_with_a_digit() {
    let keyed = [
        "https://mcp.example.com/mcp?searchApiKey=made-up-0123456789",
        " https://mcp.example.com/mcp?apikey=0123456789abcdef ",
        "https://mcp.example.com/mcp?tools=a&api_key=0123456789abcdef&b=c",
        "https://mcp.example.com/mcp?X-API-Key=0123456789abcdef",
        "https://mcp.example.com/mcp?ACCESS_TOKEN=0123456789abcdef",
        "https://mcp.example.com/mcp?client-secret=0123456789abcdef",
        "https://mcp.example.com/mcp?password=0123456789abcdef",
        "https://mcp.example.com/mcp?token=0123456789abcde=",
        "mcp.example.com?token=0123456789abcdef",
    ];
    let not_keyed = [
        "https://mcp.example.com/mcp",
        "https://mcp.example.com/mcp?",
        "https://mcp.example.com/mcp?apikey",
        "https://mcp.example.com/mcp?apikey=",
        "https://mcp.example.com/mcp?apikey=0123456789abcde",
        "https://mcp.example.com/mcp?apikey=abcdefghijklmnop",
        "https://mcp.example.com/mcp?apikeys=0123456789abcdef",
        "https://mcp.example.com/mcp?tokenizer=0123456789abcdef",
        "https://mcp.example.com/mcp?key=0123456789abcdef",
        "https://mcp.example.com/mcp?toolsets=core,apm,logs,metrics-2",
        "https://mcp.example.com/apikey=0123456789abcdef",
        "https://mcp.example.com/mcp?tools=apikey&version=0123456789abcdef",
    ];

    for typed in keyed {
        assert!(ToolServerUrl::looks_keyed(typed), "{typed}");
    }
    for typed in not_keyed {
        assert!(!ToolServerUrl::looks_keyed(typed), "{typed}");
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

#[test]
fn scopes_are_each_once_and_none_that_isnt_one() {
    let scopes = |text: &str| ToolScopes::from(text.to_owned());
    let read = scopes("read  write read \"quoted\" a\\b caf\u{e9} \u{7f}");

    assert_eq!(read.to_string(), "read write");
    assert_eq!(read.clone().and(&scopes("write admin")).to_string(), "read write admin");
    assert_eq!(read.clone().without(&scopes("admin read")).to_string(), "write");
    assert!(read.has_all(&scopes("write")) && read.has_all(&scopes("")) && !read.has_all(&scopes("write admin")));
    assert_eq!(serde_json::to_string(&read).unwrap(), "\"read write\"");
    assert_eq!(serde_json::from_str::<ToolScopes>("\"read \\\" write\"").unwrap(), read);
}

/// However a set is made, it has no more than are kept, each no longer than is kept: a server can name any number, of
/// any length, in its metadata or a 401.
#[test]
fn every_set_of_scopes_keeps_as_many_as_it_can_and_no_more() {
    let of = |numbers: std::ops::Range<usize>| numbers.map(|n| format!("scope:{n}")).collect::<Vec<_>>();
    let (longest, longer) = ("x".repeat(200), "x".repeat(201));
    let most = ToolScopes::of(of(0..32));

    let named = ToolScopes::of([of(0..40), vec![longer.clone()]].concat());
    let read = ToolScopes::from(format!("{longer} {longest} {}", of(0..40).join(" ")));
    let joined = ToolScopes::of(of(0..20)).and(&ToolScopes::of(of(10..40)));

    assert_eq!(named, most);
    assert_eq!(read.iter().count(), 32);
    assert_eq!(read.iter().next(), Some(longest.as_str()));
    assert_eq!(joined, most);
    assert_eq!(
        serde_json::from_str::<ToolScopes>(&serde_json::to_string(&of(0..40).join(" ")).unwrap()).unwrap(),
        most
    );
}

/// Where a server's own text is all read, or none of it is.
#[test]
fn what_another_machine_said_is_read_whole_where_its_all_scopes_and_few_enough_of_them() {
    let of = |count: usize| (0..count).map(|n| format!("scope:{n}")).collect::<Vec<_>>().join(" ");
    let (most, longest) = (of(32), "x".repeat(200));
    let read = [
        ("read", "read"),
        (" read  write read ", "read write"),
        (&most, &most),
        (&format!("{most} scope:0"), &most),
        (&longest, &longest),
    ];
    for (said, scopes) in read {
        assert_eq!(ToolScopes::said(said).map(String::from).as_deref(), Some(scopes), "{said:?}");
    }

    let (too_many, too_long) = (of(33), format!("read {longest}x"));
    let not_read = ["", " ", "\"", "read \"write\"", "read a\\b", "read\twrite", "read caf\u{e9}", "read \u{7f}"];
    for said in not_read.into_iter().chain([too_many.as_str(), &too_long]) {
        assert_eq!(ToolScopes::said(said), None, "{said:?}");
    }
}
