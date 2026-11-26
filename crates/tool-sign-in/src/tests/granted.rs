//! What an authorization server says it granted, for a code and for a renewal. Each sign-in here asks for `read write`.

use serde_json::{Value, json};

use super::{CLIENT, approved};
use crate::{Renewal, SaysAllowed, Script, StandIn, ToolSignIn};

/// What's read as granted of a code's answer, and of a renewal's, from an authorization server that answers each as
/// `script` says. Both give tokens all the same.
async fn read(script: Script) -> [Option<String>; 2] {
    let (client, stand_in) = (ToolSignIn::new(CLIENT), StandIn::with(script).await);
    let signed_in = approved(&client, &stand_in).await;
    let refresh = signed_in.granted.refresh.unwrap();
    let Ok(Renewal::Renewed(renewed)) = client.renew(&signed_in.renewable, &refresh).await else {
        panic!("it wasn't renewed")
    };
    [signed_in.granted.scopes, renewed.scopes].map(|scopes| scopes.map(String::from))
}

#[tokio::test]
async fn what_was_granted_is_what_its_said_to_be_whatever_was_asked_for() {
    for allows in ["read", "read write admin", "admin"] {
        let says_allowed = SaysAllowed::WhatItAllows;
        let granted = read(Script { allows: Some(allows), says_allowed, ..Script::default() }).await;

        assert_eq!(granted, [Some(allows.to_string()), Some(allows.to_string())]);
    }
}

/// What's a scope, and how many are read, is tested where `ToolScopes` is.
#[tokio::test]
async fn what_says_nothing_of_whats_granted_is_taken_for_saying_nothing_and_fails_nothing() {
    let says = [
        Value::Null,
        json!(""),
        json!(7),
        json!(["read"]),
        json!({"read": true}),
        json!("read \"write\""),
        json!("x".repeat(201)),
    ];
    for says_allowed in [SaysAllowed::Nothing].into_iter().chain(says.map(SaysAllowed::Else)) {
        let granted = read(Script { says_allowed: says_allowed.clone(), ..Script::default() }).await;

        assert_eq!(granted, [None, None], "{says_allowed:?}");
    }
}

/// Signing in again asks for it where the service lists it, whatever was granted.
#[tokio::test]
async fn what_asks_for_a_refresh_token_is_none_of_whats_granted() {
    for (said, granted) in [("read offline_access", Some("read")), ("offline_access", None)] {
        let says_allowed = SaysAllowed::Else(json!(said));
        let read = read(Script { says_allowed, ..Script::default() }).await;

        assert_eq!(read, [granted, granted].map(|granted| granted.map(str::to_string)), "{said}");
    }
}
