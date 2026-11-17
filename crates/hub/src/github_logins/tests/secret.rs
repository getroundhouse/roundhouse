//! How a GitHub account's login is kept in its file, and what a file this build didn't write reads as.

use crate::login_files::{LoginSecret as _, Stored};

use super::*;

fn signed_in() -> SignedIn {
    SignedIn {
        token: GithubToken::new("a-token"),
        token_ends_at_ms: Some(28_800_000),
        refresh: Some(crate::github::GithubRefreshToken::new("a-refresh-token")),
        refresh_ends_at_ms: None,
    }
}

/// The form is spelled out, since a build that reads it differently would take every sign-in for what isn't a login.
#[test]
fn a_token_is_kept_as_its_text_and_a_sign_in_as_a_json_object_of_its_form_and_each_reads_back_as_it_was() {
    let token = GithubLogin::Token(GithubToken::new("github_pat_made_up"));
    let sign_in = GithubLogin::SignIn(signed_in());

    let (token_bytes, sign_in_bytes) = (token.to_bytes().unwrap(), sign_in.to_bytes().unwrap());

    assert_eq!(token_bytes, b"github_pat_made_up");
    let form = r#"{"format":"sign_in","token":"a-token","token_ends_at_ms":28800000,"refresh":"a-refresh-token","refresh_ends_at_ms":null}"#;
    assert_eq!(String::from_utf8(sign_in_bytes.clone()).unwrap(), form);
    assert_eq!(GithubLogin::from_bytes(token_bytes), Stored::Secret(token));
    assert_eq!(GithubLogin::from_bytes(sign_in_bytes), Stored::Secret(sign_in));
}

/// An installation's token, and an app's key, are spelled out too, for the same reason.
#[test]
fn an_installations_token_and_an_apps_key_are_kept_as_json_objects_of_their_forms_and_read_back_as_they_were() {
    let token = InstallationToken { token: GithubToken::new("ghs_made_up"), token_ends_at_ms: 3_600_000 };
    let (installation, app) =
        (GithubLogin::Installation(token), GithubLogin::App(crate::github::GithubAppKey::new("a-made-up-key")));

    let (installation_bytes, app_bytes) = (installation.to_bytes().unwrap(), app.to_bytes().unwrap());

    let form = r#"{"format":"installation","token":"ghs_made_up","token_ends_at_ms":3600000}"#;
    assert_eq!(String::from_utf8(installation_bytes.clone()).unwrap(), form);
    assert_eq!(String::from_utf8(app_bytes.clone()).unwrap(), r#"{"format":"app","key":"a-made-up-key"}"#);
    assert_eq!(GithubLogin::from_bytes(installation_bytes), Stored::Secret(installation));
    assert_eq!(GithubLogin::from_bytes(app_bytes), Stored::Secret(app));
}

/// A field a newer build added is read past, a form a newer build knows is kept for it, and a JSON object that's no
/// form, or one cut short, holds none, as bytes that aren't text don't.
#[test]
fn a_form_from_a_newer_build_is_kept_and_what_isnt_a_form_holds_nothing() {
    let read = |bytes: &[u8]| GithubLogin::from_bytes(bytes.to_vec());
    let with_more = br#"{"format":"sign_in","token":"a-token","scopes_asked":["repo"]}"#;

    let with_more = read(with_more);

    let expected = SignedIn { token_ends_at_ms: None, refresh: None, ..signed_in() };
    assert_eq!(with_more, Stored::Secret(GithubLogin::SignIn(expected)));
    assert_eq!(read(br#"{"format":"sign_in_with_a_passkey","passkey":"made-up"}"#), Stored::Newer);
    assert_eq!(read(br#"{"token":"a-token"}"#), Stored::Nothing);
    assert_eq!(read(br#"{"format":"sign_in","tok"#), Stored::Nothing);
    assert_eq!(read(&[0xff, 0xfe]), Stored::Nothing);
}
