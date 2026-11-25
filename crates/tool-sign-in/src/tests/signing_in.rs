use crate::{Found, RedirectUri, Returned, SignedIn, StandIn, Started, ToolSignIn};

pub(crate) const CLIENT: &str = "https://client.example/client.json";
pub(crate) const REDIRECT: &str = "http://127.0.0.1:17867/callback";

/// A sign-in to `stand_in` that the person hasn't approved yet, asking for what its server asks for.
pub(crate) async fn begun(client: &ToolSignIn, stand_in: &StandIn) -> Started {
    let found = client.find(&stand_in.server()).await.unwrap();
    let Found::SignIn(way) = found else { panic!("{found:?}") };
    let scopes = way.scopes().clone();
    client.begin(*way, scopes, RedirectUri::parse(REDIRECT).unwrap()).unwrap()
}

/// One that they approved.
pub(crate) async fn approved(client: &ToolSignIn, stand_in: &StandIn) -> SignedIn {
    let Started { page, pending } = begun(client, stand_in).await;
    let query = stand_in.visit(&page);
    client.finish(&pending, Returned::of_query(&query)).await.unwrap()
}
