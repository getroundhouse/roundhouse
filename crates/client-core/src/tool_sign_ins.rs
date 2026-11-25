//! What signing in to a tool server comes to, in words for a person: why its server has no page to sign in on, and why
//! it couldn't be started or finished. "MCP servers" in `docs/spec/settings.md` plans the panels that will say it.

use protocol::client::ErrorCode;
use protocol::client::tool_sign_ins::{NoToolSignIn, ToolSignInBegun, ToolSignInPage};

use crate::ClientError;
use crate::refusals::{couldnt, gone};

/// What a start comes to, for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Begun {
    /// The page to open in their browser.
    Page(ToolSignInPage),
    /// There's none, and why, for a client to say beside its offer of a key.
    NoPage(String),
}

/// What `begun` comes to.
pub fn begun(begun: ToolSignInBegun) -> Begun {
    Begun::NoPage(match begun {
        ToolSignInBegun::Page { page } => return Begun::Page(page),
        ToolSignInBegun::NoSignIn => "This server has no sign-in. Anyone who can reach it can use it.".to_string(),
        ToolSignInBegun::NoWay { why } => no_way(&why),
        ToolSignInBegun::Unknown => "Update this app to sign in to this server.".to_string(),
    })
}

fn no_way(why: &NoToolSignIn) -> String {
    let said = match why {
        NoToolSignIn::NoAuthorizationServer => "It asks for a sign-in, and doesn't say where to sign in.".to_string(),
        NoToolSignIn::OnlyRegisteredClients { authorization_server } => {
            format!("{authorization_server}, where it's signed in to, takes only apps registered with it.")
        }
        NoToolSignIn::OtherServer { said } => format!("It says its sign-in is for {said}, which isn't its address."),
        NoToolSignIn::NoS256 { authorization_server } => {
            format!("{authorization_server}, where it's signed in to, doesn't say it keeps a sign-in safe with PKCE.")
        }
        NoToolSignIn::Unknown => return "Roundhouse can't sign in to this server.".to_string(),
    };
    format!("Roundhouse can't sign in to this server. {said}")
}

/// What to say when a sign-in couldn't be started, for `e`.
pub fn couldnt_start(e: &ClientError) -> String {
    // The server to sign in to again.
    if is_gone(e) { gone("MCP server") } else { couldnt("sign in", e) }
}

/// What to say when a sign-in couldn't be finished, for `e`. One the hub no longer has is started again.
pub fn couldnt_finish(e: &ClientError) -> String {
    if is_gone(e) { format!("{} Start it again.", gone("sign-in")) } else { couldnt("sign in", e) }
}

fn is_gone(e: &ClientError) -> bool {
    matches!(e, ClientError::Api(api) if api.code == ErrorCode::NotFound)
}

#[cfg(test)]
mod tests {
    use protocol::client::ApiError;

    use super::*;

    #[test]
    fn a_start_is_a_page_or_why_there_is_none() {
        let page = ToolSignInPage::new("https://auth.example.com");
        let said = |why| begun(ToolSignInBegun::NoWay { why });
        let no_page = |said: &str| Begun::NoPage(said.to_string());
        let authorization_server = || "https://auth.example.com".to_string();

        assert_eq!(begun(ToolSignInBegun::Page { page: page.clone() }), Begun::Page(page));
        assert_eq!(
            begun(ToolSignInBegun::NoSignIn),
            no_page("This server has no sign-in. Anyone who can reach it can use it.")
        );
        assert_eq!(
            said(NoToolSignIn::NoAuthorizationServer),
            no_page(
                "Roundhouse can't sign in to this server. It asks for a sign-in, and doesn't say where to sign in."
            )
        );
        assert_eq!(
            said(NoToolSignIn::OnlyRegisteredClients { authorization_server: authorization_server() }),
            no_page(
                "Roundhouse can't sign in to this server. https://auth.example.com, where it's signed in to, takes only \
                 apps registered with it."
            )
        );
        assert_eq!(
            said(NoToolSignIn::OtherServer { said: "https://other.example".into() }),
            no_page(
                "Roundhouse can't sign in to this server. It says its sign-in is for https://other.example, which isn't \
                 its address."
            )
        );
        assert_eq!(
            said(NoToolSignIn::NoS256 { authorization_server: authorization_server() }),
            no_page(
                "Roundhouse can't sign in to this server. https://auth.example.com, where it's signed in to, doesn't say \
                 it keeps a sign-in safe with PKCE."
            )
        );
        assert_eq!(said(NoToolSignIn::Unknown), no_page("Roundhouse can't sign in to this server."));
        assert_eq!(begun(ToolSignInBegun::Unknown), no_page("Update this app to sign in to this server."));
    }

    #[test]
    fn a_sign_in_the_hub_no_longer_has_is_started_again_and_any_other_refusal_is_the_hubs_own() {
        let refused = |code| ClientError::Api(ApiError::new(code, "the sign-in was refused"));
        let hubs_own = "Couldn't sign in: the sign-in was refused";

        assert_eq!(
            couldnt_finish(&refused(ErrorCode::NotFound)),
            "The hub no longer has this sign-in. Start it again."
        );
        assert_eq!(couldnt_finish(&refused(ErrorCode::InvalidRequest)), hubs_own);
        assert_eq!(couldnt_start(&refused(ErrorCode::NotFound)), "The hub no longer has this MCP server.");
        assert_eq!(couldnt_start(&refused(ErrorCode::Unavailable)), hubs_own);
        assert_eq!(couldnt_start(&ClientError::Disconnected), "Couldn't sign in: the connection to the hub was lost");
    }
}
