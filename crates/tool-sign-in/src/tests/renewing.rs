use protocol::client::{ApiError, ErrorCode};

use super::{CLIENT, approved};
use crate::{Granted, RefreshToken, Refreshing, Renewable, Renewal, Script, SignedIn, StandIn, ToolSignIn};

/// A sign-in to a stand-in answering as `script` says, with what renews it and its refresh token.
async fn signed_in(script: Script) -> (ToolSignIn, StandIn, Renewable, RefreshToken) {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(script).await);
    let SignedIn { renewable, granted, .. } = approved(&client, &stand_in).await;
    (client, stand_in, renewable, granted.refresh.unwrap())
}

/// What renewing gives.
async fn renewed(client: &ToolSignIn, renewable: &Renewable, refresh: &RefreshToken) -> Granted {
    let renewal = client.renew(renewable, refresh).await.unwrap();
    let Renewal::Renewed(granted) = renewal else { panic!("{renewal:?}") };
    granted
}

/// What's kept of one is read back as another hub's process reads it.
#[tokio::test]
async fn a_refresh_token_thats_replaced_is_refused_from_then_on_and_the_one_in_its_place_renews() {
    let (client, stand_in, renewable, first) = signed_in(Script::default()).await;
    let kept = serde_json::to_string(&renewable).unwrap();
    let renewable: Renewable = serde_json::from_str(&kept).unwrap();

    let second = renewed(&client, &renewable, &first).await.refresh.unwrap();
    let reused = client.renew(&renewable, &first).await.unwrap();
    let third = renewed(&client, &renewable, &second).await;

    assert_ne!(first, second);
    let Renewal::SignInAgain(error) = reused else { panic!("{reused:?}") };
    assert_eq!(error.as_str(), "invalid_grant");
    assert!(third.lasts.is_some());
    let renewals = stand_in.asked_for("POST /token").split_off(1);
    assert!(renewals.iter().all(|asked| asked.form("resource").as_deref() == Some(stand_in.server().as_str())));
}

#[tokio::test]
async fn a_refresh_token_that_isnt_replaced_goes_on_renewing() {
    let (client, _stand_in, renewable, first) = signed_in(Script { rotates: false, ..Script::default() }).await;

    let again = renewed(&client, &renewable, &first).await;

    assert_eq!(again.refresh.as_ref(), Some(&first));
    assert_eq!(renewed(&client, &renewable, &first).await.refresh, Some(first));
}

/// `invalid_client` is for good only of a registered client, whose registration the authorization server holds, as
/// `registering`'s tests have it. Of the client document, it's what a service says that can't fetch the document for
/// now.
#[tokio::test]
async fn a_refusal_for_good_means_signing_in_again_and_anything_else_may_pass() {
    let (client, stand_in, renewable, refresh) = signed_in(Script::default()).await;

    stand_in.change(|script| script.refreshing = Refreshing::Refused("invalid_grant"));
    let renewal = client.renew(&renewable, &refresh).await.unwrap();
    let Renewal::SignInAgain(said) = renewal else { panic!("{renewal:?}") };
    assert_eq!(said.as_str(), "invalid_grant");
    let for_now = [
        (Refreshing::Refused("invalid_client"), ErrorCode::InvalidRequest),
        (Refreshing::Answers(503), ErrorCode::Unavailable),
        (Refreshing::Answers(429), ErrorCode::Unavailable),
        (Refreshing::Answers(307), ErrorCode::InvalidRequest),
        (Refreshing::Answers(200), ErrorCode::InvalidRequest),
        (Refreshing::Refused("invalid_request"), ErrorCode::InvalidRequest),
        (Refreshing::Refused("temporarily_unavailable"), ErrorCode::InvalidRequest),
    ];
    for (refreshing, code) in for_now {
        stand_in.change(|script| script.refreshing = refreshing);
        let failure = client.renew(&renewable, &refresh).await.unwrap_err();
        assert_eq!(ApiError::from(failure).code, code, "{refreshing:?}");
    }
    assert!(stand_in.asked_for("POST /moved").is_empty(), "it has the refresh token in it");
    stand_in.change(|script| script.refreshing = Refreshing::Works);
    renewed(&client, &renewable, &refresh).await;
}
