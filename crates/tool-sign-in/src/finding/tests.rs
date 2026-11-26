use std::time::Duration;

use http::header::COOKIE;
use protocol::model::ToolScopes;
use tokio::time::Instant;
use url::Url;

use super::{FIND_WITHIN, of_authorization_server, of_server};
use crate::tests::CLIENT;
use crate::{
    Asks, Failure, Found, Issuer, MetadataAt, NoWay, Says, Script, ServerMetadataAt, StandIn, ToolSignIn, Way,
};

/// Far longer than a stand-in takes to be asked, on a busy machine.
const ASKED_WITHIN: Duration = Duration::from_secs(5);

/// What `stand_in`'s server is found to take.
async fn find(stand_in: &StandIn) -> Result<Found, Failure> {
    ToolSignIn::new(CLIENT).find(&stand_in.server()).await
}

async fn found_with(script: Script) -> Result<Found, Failure> {
    find(&StandIn::with(script).await).await
}

async fn way_with(script: Script) -> Way {
    let found = found_with(script).await.unwrap();
    let Found::SignIn(way) = found else { panic!("{found:?}") };
    *way
}

fn scopes(text: &str) -> ToolScopes {
    ToolScopes::from(text.to_string())
}

/// One that answers anyone is looked at in the same places, but for where a 401 says.
#[tokio::test]
async fn a_server_thats_signed_in_to_says_where_in_its_401_or_at_its_paths_or_its_hosts_well_known_place() {
    let well_known = "GET /.well-known/oauth-protected-resource";
    let its_host = vec![format!("{well_known}/mcp"), well_known.to_string()];
    let cases = [
        (Asks::SignIn, ServerMetadataAt::Said, vec!["GET /metadata".to_string()], "/mcp"),
        (Asks::SignIn, ServerMetadataAt::ItsPath, vec![format!("{well_known}/mcp")], "/mcp"),
        (Asks::SignIn, ServerMetadataAt::ItsHost, its_host.clone(), ""),
        (Asks::Nothing, ServerMetadataAt::ItsPath, vec![format!("{well_known}/mcp")], "/mcp"),
        (Asks::Nothing, ServerMetadataAt::ItsHost, its_host, ""),
    ];
    for (asks, server_metadata_at, looked, resource) in cases {
        let stand_in = StandIn::with(Script { asks, server_metadata_at, ..Script::default() }).await;

        let found = find(&stand_in).await.unwrap();

        let Found::SignIn(way) = found else { panic!("{asks:?}, {server_metadata_at:?}: {found:?}") };
        assert_eq!(way.resource, format!("{}{resource}", stand_in.issuer()));
        assert_eq!(way.issuer().as_str(), stand_in.issuer());
        assert_eq!(way.server, stand_in.server());
        let asked = stand_in.asked();
        let lines: Vec<_> = asked.iter().map(|asked| asked.line.clone()).collect();
        let first = ["POST /mcp".to_string()];
        let last = ["GET /.well-known/oauth-authorization-server".to_string()];
        assert_eq!(lines, [&first[..], &looked, &last].concat(), "{asks:?}, {server_metadata_at:?}");
        assert!(asked.iter().all(|asked| !asked.headers.contains_key(COOKIE)), "a cookie was sent");
    }
}

/// What's not there, or can't be read, says nothing of a server that answers anyone, and is why one that asks can't be
/// signed in to.
#[tokio::test]
async fn a_server_that_answers_anyone_and_one_that_asks_with_no_way_to_are_told_apart() {
    let at_its_path = Script { server_metadata_at: ServerMetadataAt::ItsPath, ..Script::default() };
    let server = |server_metadata_at| Script { server_metadata_at, ..Script::default() };
    let authorization = |metadata_at| Script { metadata_at, ..at_its_path.clone() };
    let nothing = [server(ServerMetadataAt::Nowhere), authorization(MetadataAt::Nowhere)];
    let unread = [server(ServerMetadataAt::Answers(503)), authorization(MetadataAt::Answers(503))];

    for script in nothing {
        let asks = found_with(script.clone()).await.unwrap();
        assert!(matches!(asks, Found::NoWay(NoWay::NoAuthorizationServer)), "{asks:?}");
        let anyone = found_with(Script { asks: Asks::Nothing, ..script }).await.unwrap();
        assert!(matches!(anyone, Found::NoSignIn), "{anyone:?}");
    }
    for script in unread {
        let asks = found_with(script.clone()).await.unwrap_err();
        assert!(matches!(asks, Failure::Busy { status: 503, .. }), "{asks:?}");
        let anyone = found_with(Script { asks: Asks::Nothing, ..script }).await.unwrap();
        assert!(matches!(anyone, Found::NoSignIn), "{anyone:?}");
    }
}

/// Metadata that's there says there's a sign-in, whether or not the server asks for one.
#[tokio::test]
async fn a_server_whose_sign_in_isnt_one_roundhouse_gives_says_why_and_metadata_of_another_issuer_is_refused() {
    for asks in [Asks::SignIn, Asks::Nothing] {
        let script = Script { asks, server_metadata_at: ServerMetadataAt::ItsPath, ..Script::default() };
        let registered = StandIn::with(Script { client_documents: false, ..script.clone() }).await;
        let s256 = StandIn::with(Script { s256: false, ..script.clone() }).await;
        let resource = found_with(Script { resource: Says::Another, ..script.clone() }).await.unwrap();
        let issuer = found_with(Script { issuer: Says::Another, ..script }).await.unwrap_err();

        let Found::NoWay(resource) = resource else { panic!("{resource:?}") };
        let other = "https://other.example/mcp";
        assert_eq!(resource, NoWay::OtherResource { said: other.to_string() });
        let said = "the server says its sign-in is for https://other.example/mcp, which isn't its address";
        assert_eq!(resource.to_string(), said);
        for (stand_in, no_way) in [(&registered, NoWay::OnlyRegisteredClients as fn(_) -> _), (&s256, NoWay::NoS256)] {
            let found = find(stand_in).await.unwrap();
            let Found::NoWay(found) = found else { panic!("{found:?}") };
            assert_eq!(found, no_way(Issuer::new(stand_in.issuer())));
        }
        assert!(matches!(issuer, Failure::OtherIssuer { .. }), "{issuer:?}");
    }
}

/// Time stands still once the authorization server has been asked, and moves only by tokio's paused clock, so nothing
/// is waited out. Each of its three places would be waited for as long as one call may take, which is longer in all.
#[tokio::test]
async fn looking_ends_at_its_time_limit_and_a_server_that_answers_anyone_is_then_taken_to_have_no_sign_in() {
    for asks in [Asks::SignIn, Asks::Nothing] {
        let script = Script { asks, issuer_path: "/tenant", ..Script::default() };
        let script = Script { server_metadata_at: ServerMetadataAt::ItsPath, ..script };
        let stand_in = StandIn::with(Script { metadata_at: MetadataAt::NeverAnswers, ..script }).await;
        let server = stand_in.server();
        let started = Instant::now();
        let looking = tokio::spawn(async move { ToolSignIn::new(CLIENT).find(&server).await });
        let asked = || !stand_in.asked_for("GET /.well-known/oauth-authorization-server/tenant").is_empty();
        waiting::task::until(ASKED_WITHIN, "the authorization server wasn't asked", asked).await;

        tokio::time::pause();
        let found = looking.await.unwrap();
        tokio::time::resume();

        match asks {
            Asks::SignIn => assert!(matches!(found, Err(Failure::TimedOut { .. })), "{found:?}"),
            Asks::Nothing | Asks::Answers(_) => assert!(matches!(found, Ok(Found::NoSignIn)), "{found:?}"),
        }
        let took = started.elapsed();
        assert!(took >= FIND_WITHIN && took < FIND_WITHIN + Duration::from_secs(10), "it took {took:?}");
    }
}

/// One on either side of MCP's handshake refuses the other's first message, some before they ask for a sign-in.
#[tokio::test]
async fn a_server_is_asked_as_by_a_client_of_either_side_of_the_handshake_before_its_taken_not_to_ask() {
    for (asks, refuses, asked) in [
        (Asks::SignIn, None, 1),
        (Asks::SignIn, Some("server/discover"), 2),
        (Asks::SignIn, Some("initialize"), 1),
        (Asks::Nothing, Some("server/discover"), 2),
        (Asks::Nothing, Some("initialize"), 1),
    ] {
        let stand_in = StandIn::with(Script { asks, refuses_method: refuses, ..Script::default() }).await;

        let found = find(&stand_in).await.unwrap();

        assert_eq!(matches!(found, Found::SignIn(_)), asks == Asks::SignIn, "{found:?}, refusing {refuses:?}");
        assert_eq!(stand_in.asked_for("POST /mcp").len(), asked, "refusing {refuses:?}");
    }
}

#[tokio::test]
async fn a_server_that_neither_answers_nor_asks_is_a_failure_which_may_pass_where_its_down() {
    let not_one = found_with(Script { asks: Asks::Answers(404), ..Script::default() }).await.unwrap_err();
    let down = found_with(Script { asks: Asks::Answers(503), ..Script::default() }).await.unwrap_err();

    let moved = StandIn::with(Script { asks: Asks::Answers(307), ..Script::default() }).await;
    let has_moved = find(&moved).await.unwrap_err();

    assert!(matches!(has_moved, Failure::Answered { status: 307, .. }), "{has_moved:?}");
    assert!(moved.asked_for("POST /moved").is_empty());
    assert!(matches!(not_one, Failure::Answered { status: 404, .. }), "{not_one:?}");
    assert!(matches!(down, Failure::Busy { status: 503, .. }), "{down:?}");
}

#[tokio::test]
async fn an_authorization_servers_metadata_is_found_in_each_place_mcp_has_it_in_its_order() {
    let (oauth, openid) = ("GET /.well-known/oauth-authorization-server", "GET /.well-known/openid-configuration");
    let cases = [
        (MetadataAt::Oauth, "", vec![oauth.to_string()]),
        (MetadataAt::OpenId, "", vec![oauth.to_string(), openid.to_string()]),
        (MetadataAt::Oauth, "/tenant", vec![format!("{oauth}/tenant")]),
        (MetadataAt::OpenId, "/tenant", vec![format!("{oauth}/tenant"), format!("{openid}/tenant")]),
        (
            MetadataAt::OpenIdAppended,
            "/tenant",
            vec![
                format!("{oauth}/tenant"),
                format!("{openid}/tenant"),
                "GET /tenant/.well-known/openid-configuration".to_string(),
            ],
        ),
    ];
    for (metadata_at, issuer_path, looked) in cases {
        let stand_in = StandIn::with(Script { issuer_path, metadata_at, ..Script::default() }).await;

        let Found::SignIn(way) = find(&stand_in).await.unwrap() else { panic!("{metadata_at:?} wasn't found") };

        assert_eq!(way.issuer().as_str(), stand_in.issuer());
        let lines: Vec<_> = stand_in.asked().into_iter().skip(2).map(|asked| asked.line).collect();
        assert_eq!(lines, looked, "{metadata_at:?} of {issuer_path}");
    }
}

#[tokio::test]
async fn what_to_ask_to_be_allowed_is_what_the_401_says_or_else_the_metadata_and_a_refresh_token_where_there_is_one() {
    let (said, supported) = (Some("read write"), Some(vec!["read", "admin"]));
    let offline = Some(vec!["openid", "offline_access"]);
    let cases = [
        (said, supported.clone(), None, "read write", false),
        (None, supported, None, "read admin", false),
        (None, None, None, "", false),
        (None, None, offline, "", true),
    ];
    for (challenge_scope, server_scopes, authorization_scopes, asked, offline) in cases {
        let script = Script { challenge_scope, server_scopes, authorization_scopes, ..Script::default() };
        let way = way_with(script.clone()).await;
        assert_eq!((way.scopes(), way.offline), (&scopes(asked), offline), "{script:?}");
    }
}

#[test]
fn well_known_places_are_made_of_an_addresss_path() {
    let url = |text: &str| Url::parse(text).unwrap();
    let texts = |urls: Vec<Url>| urls.into_iter().map(String::from).collect::<Vec<_>>();
    let root = "https://tools.example/.well-known/oauth-protected-resource";

    assert_eq!(texts(of_server(&url("https://tools.example"))), [root]);
    assert_eq!(texts(of_server(&url("https://tools.example/a/mcp"))), [format!("{root}/a/mcp"), root.to_string()]);
    assert_eq!(texts(of_server(&url("https://tools.example/mcp?a=b"))), [format!("{root}/mcp?a=b"), root.to_string()]);
    assert_eq!(
        texts(of_authorization_server(&url("https://sign-in.example/tenant/"))),
        [
            "https://sign-in.example/.well-known/oauth-authorization-server/tenant",
            "https://sign-in.example/.well-known/openid-configuration/tenant",
            "https://sign-in.example/tenant/.well-known/openid-configuration",
        ]
    );
}

#[test]
fn metadata_is_of_a_server_by_its_address_but_for_how_its_host_is_written() {
    let is_of = |resource: &str, server: &str| {
        let metadata = serde_json::json!({ "resource": resource });
        let metadata: super::metadata::ServerMetadata = serde_json::from_value(metadata).unwrap();
        metadata.is_of(&Url::parse(server).unwrap())
    };

    for same in ["https://tools.example", "https://tools.example/", "HTTPS://Tools.Example:443"] {
        assert!(is_of(same, "https://tools.example"), "{same}");
    }
    for other in
        ["https://tools.example/mcp/", "https://tools.example", "https://tools.example/MCP", "http://tools.example/mcp"]
    {
        assert!(!is_of(other, "https://tools.example/mcp"), "{other}");
    }
}
