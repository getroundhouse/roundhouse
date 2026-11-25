use super::{CLIENT, approved, begun};
use crate::{Returned, Script, StandIn, Started, ToolSignIn};

/// Each is held as a token's type, so whatever holds one can be logged whole.
#[tokio::test]
async fn no_secret_is_in_the_debug_output_of_what_holds_it() {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(Script::default()).await);

    let started = begun(&client, &stand_in).await;
    let logged = format!("{started:?}");
    let Started { page, pending } = started;
    let query = stand_in.visit(&page);
    let returned = Returned::of_query(&query);
    let logged = format!("{logged} {returned:?}");
    let signed_in = client.finish(&pending, returned).await.unwrap();
    let logged = format!("{logged} {signed_in:?}");

    let exchange = stand_in.asked_for("POST /token").remove(0);
    let (code, verifier) = (exchange.form("code").unwrap(), exchange.form("code_verifier").unwrap());
    let state = query.split("state=").nth(1).unwrap().split('&').next().unwrap().to_string();
    let access = signed_in.granted.access.as_str().to_string();
    let refresh = signed_in.granted.refresh.unwrap().as_str().to_string();
    for secret in [code, verifier, state, access, refresh] {
        assert!(!logged.contains(&secret), "{secret} is in {logged}");
    }
    let kept = serde_json::to_string(&signed_in.renewable).unwrap();
    assert!(!kept.contains("made-up-"), "{kept}");
}

/// The tokens given are ones the server takes, as the hub's tests rely on, until they're ended.
#[tokio::test]
async fn the_stand_ins_server_takes_the_tokens_its_authorization_server_gives_until_theyre_ended() {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(Script::default()).await);
    let granted = approved(&client, &stand_in).await.granted;
    let calls = client.calls_for(&stand_in.server()).await.unwrap();
    let bearer = format!("Bearer {}", granted.access.as_str());
    let message = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let server = stand_in.server();
    let call = async || calls.post_message(server.url(), &[("authorization", &bearer)], &message).await;

    let taken = call().await.unwrap().status;
    stand_in.end_access_tokens();
    let refused = call().await.unwrap().status;

    assert_eq!((taken, refused), (200, 401));
}
