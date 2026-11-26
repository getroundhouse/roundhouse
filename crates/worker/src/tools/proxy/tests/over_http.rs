//! A server signed in to over plain `http`, whose name comes to an address on the internet, or a private one.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use http_body_util::{Either, Full};
use hyper::{Response, StatusCode};
use protocol::worker::ProxiedServer;

use super::super::private::Lookup;
use super::signed_in::{CALL, FIRST, with};
use super::stand_in::url;
use super::{Agent, StandIn};

const NOT_PRIVATE: &str = "Linear (Northwind)'s address isn't private, so its sign-in isn't sent to it over http. Give \
                           it an https URL in Settings › MCP servers.";

/// On the internet, as `protocol`'s rule says, so a test that's broken on purpose dials one, and neither answered when
/// that was tried, so it fails once connecting times out.
const PUBLIC: [&str; 2] = ["11.0.0.1", "3ffe::1"];

fn lookup() -> Lookup {
    let public = PUBLIC.map(|address| address.parse::<IpAddr>().unwrap());
    let private = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let names = [
        ("tools.northwind.test", vec![private]),
        ("both.northwind.test", vec![public[0], public[1], private]),
        ("public.northwind.test", public.to_vec()),
    ];
    Lookup::Fixed(Arc::new(HashMap::from(names)))
}

fn at(server: &StandIn, host: &str) -> ProxiedServer {
    let url = url(&format!("http://{host}:{}/mcp", server.port));
    ProxiedServer { url, ..with(server, "made-up-token", FIRST) }
}

#[tokio::test]
async fn a_sign_in_goes_over_http_only_to_a_private_address_of_its_servers() {
    let server = StandIn::answering(|asked| {
        Response::builder().status(StatusCode::OK).body(Either::Left(Full::new(asked.body.clone()))).unwrap()
    })
    .await;
    let cases = [
        ("tools.northwind.test", true),
        ("both.northwind.test", true),
        ("public.northwind.test", false),
        ("11.0.0.1", false),
        ("[3ffe::1]", false),
    ];

    for (host, private) in cases {
        let heard = server.heard().len();
        let agent = Agent::looking_up(vec![at(&server, host)], lookup()).await;

        let answer = agent.post(CALL).await;

        if private {
            assert_eq!((answer.status, answer.body.as_str()), (StatusCode::OK, CALL), "{host}");
            let sent = &server.heard()[heard].headers["authorization"];
            assert_eq!((server.heard().len(), sent.to_str().unwrap()), (heard + 1, "Bearer made-up-token"), "{host}");
        } else {
            assert_eq!(answer.error().1, NOT_PRIVATE, "{host}");
            assert_eq!(server.heard().len(), heard, "{host}");
        }
    }
}

#[tokio::test]
async fn a_request_with_no_id_is_told_its_server_isnt_private_with_a_502() {
    let server = StandIn::answering(|_| unreachable!()).await;
    let agent = Agent::looking_up(vec![at(&server, "public.northwind.test")], lookup()).await;

    let answer = agent.get().await;

    assert_eq!((answer.status, answer.body.as_str()), (StatusCode::BAD_GATEWAY, NOT_PRIVATE));
}
