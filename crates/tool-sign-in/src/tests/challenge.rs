use http::HeaderMap;
use http::header::WWW_AUTHENTICATE;
use protocol::model::ToolScopes;

use crate::access_wanted;
use crate::challenge::Challenge;

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
