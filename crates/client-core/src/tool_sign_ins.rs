//! What signing in to a tool server comes to, in words for a person: where it's approved, why its server has no page to
//! sign in on, and why it couldn't be started or finished.

use protocol::client::ErrorCode;
use protocol::client::tool_sign_ins::{NoToolSignIn, ToolSignInBegun, ToolSignInPage};

use crate::ClientError;
use crate::refusals::{couldnt, gone};

/// What a start comes to, for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Begun {
    /// The page to open in their browser.
    Page(ToolSignInPage),
    /// There's none, for this reason.
    NoPage(NoPage),
}

/// Why there's no page, which says what a client can offer in its place for a server that isn't added yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoPage {
    /// The server has no sign-in, since anyone can use it.
    NoSignIn,
    /// It can't be signed in to, as this says, for a person.
    CantSignIn(String),
}

impl NoPage {
    /// Why, for a person.
    pub fn why(&self) -> &str {
        match self {
            NoPage::NoSignIn => "This server has no sign-in. Anyone who can reach it can use it.",
            NoPage::CantSignIn(why) => why,
        }
    }
}

/// What `begun` comes to.
pub fn begun(begun: ToolSignInBegun) -> Begun {
    Begun::NoPage(match begun {
        ToolSignInBegun::Page { page } => return Begun::Page(page),
        ToolSignInBegun::NoSignIn => NoPage::NoSignIn,
        ToolSignInBegun::NoWay { why } => NoPage::CantSignIn(no_way(&why)),
        ToolSignInBegun::Unknown => NoPage::CantSignIn("Update this app to sign in to this server.".to_string()),
    })
}

fn no_way(why: &NoToolSignIn) -> String {
    let said = match why {
        NoToolSignIn::NoAuthorizationServer => "It asks for a sign-in, and doesn't say where to sign in.".to_string(),
        NoToolSignIn::OnlyRegisteredClients { host } => {
            format!("{host}, where it's signed in to, takes only apps registered with it.")
        }
        NoToolSignIn::OnlyClientsWithSecrets { host } => {
            format!("{host}, where it's signed in to, takes only apps with a secret, and Roundhouse has none.")
        }
        NoToolSignIn::NotRegistered { host, error } => {
            let why = error.as_ref().map_or_default(|error| format!(": {error}"));
            format!("{host}, where it's signed in to, wouldn't register Roundhouse{why}.")
        }
        NoToolSignIn::OtherServer { host } if host.is_empty() => {
            "It says its sign-in is for another server.".to_string()
        }
        NoToolSignIn::OtherServer { host } => format!("It says its sign-in is for {host}, which isn't its address."),
        NoToolSignIn::NoS256 { host } => {
            format!("{host}, where it's signed in to, doesn't say it keeps a sign-in safe with PKCE.")
        }
        NoToolSignIn::Unknown => return "Roundhouse can't sign in to this server.".to_string(),
    };
    format!("Roundhouse can't sign in to this server. {said}")
}

/// What a sign-in says while the person has its page: to approve Roundhouse there, on `page`, the page's host, for the
/// server at `server`, its host with its port if it has one, in the browser on this `machine`, such as `Mac`. A server
/// can send the person to any service's page, so both are named, as `on auth.linear.app for mcp.example.com`, and the
/// host once where the server is on the page's, whatever its port.
pub fn approve_on(page: &str, server: &str, machine: &str) -> String {
    let on = if same_host(page, server) { page.to_string() } else { format!("{page} for {server}") };
    format!("Approve Roundhouse on {on}, in the browser on this {machine}. Roundhouse never sees your password.")
}

/// Whether `page`, a host, is the host of `server`, a host with or without its port, in any case, as hosts are.
fn same_host(page: &str, server: &str) -> bool {
    let port = |port: &str| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit());
    let named = server.rsplit_once(':').filter(|(_, after)| port(after)).map_or(server, |(named, _)| named);
    page.eq_ignore_ascii_case(named)
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
        let cant_sign_in = |said: &str| Begun::NoPage(NoPage::CantSignIn(said.to_string()));
        let host = || "auth.example.com".to_string();

        assert_eq!(begun(ToolSignInBegun::Page { page: page.clone() }), Begun::Page(page));
        assert_eq!(begun(ToolSignInBegun::NoSignIn), Begun::NoPage(NoPage::NoSignIn));
        assert_eq!(NoPage::NoSignIn.why(), "This server has no sign-in. Anyone who can reach it can use it.");
        assert_eq!(NoPage::CantSignIn("Why".into()).why(), "Why");
        assert_eq!(
            said(NoToolSignIn::NoAuthorizationServer),
            cant_sign_in(
                "Roundhouse can't sign in to this server. It asks for a sign-in, and doesn't say where to sign in."
            )
        );
        assert_eq!(
            said(NoToolSignIn::OnlyRegisteredClients { host: host() }),
            cant_sign_in(
                "Roundhouse can't sign in to this server. auth.example.com, where it's signed in to, takes only \
                 apps registered with it."
            )
        );
        assert_eq!(
            said(NoToolSignIn::OnlyClientsWithSecrets { host: host() }),
            cant_sign_in(
                "Roundhouse can't sign in to this server. auth.example.com, where it's signed in to, takes only \
                 apps with a secret, and Roundhouse has none."
            )
        );
        for (error, why) in [(Some("invalid_redirect_uri".to_string()), ": invalid_redirect_uri"), (None, "")] {
            let begun = said(NoToolSignIn::NotRegistered { host: host(), error });
            let expected = format!(
                "Roundhouse can't sign in to this server. auth.example.com, where it's signed in to, wouldn't \
                 register Roundhouse{why}."
            );
            assert_eq!(begun, cant_sign_in(&expected));
        }
        assert_eq!(
            said(NoToolSignIn::OtherServer { host: "other.example".into() }),
            cant_sign_in(
                "Roundhouse can't sign in to this server. It says its sign-in is for other.example, which isn't its \
                 address."
            )
        );
        assert_eq!(
            said(NoToolSignIn::OtherServer { host: String::new() }),
            cant_sign_in("Roundhouse can't sign in to this server. It says its sign-in is for another server.")
        );
        assert_eq!(
            said(NoToolSignIn::NoS256 { host: host() }),
            cant_sign_in(
                "Roundhouse can't sign in to this server. auth.example.com, where it's signed in to, doesn't say \
                 it keeps a sign-in safe with PKCE."
            )
        );
        assert_eq!(said(NoToolSignIn::Unknown), cant_sign_in("Roundhouse can't sign in to this server."));
        assert_eq!(begun(ToolSignInBegun::Unknown), cant_sign_in("Update this app to sign in to this server."));
    }

    #[test]
    fn a_sign_in_is_approved_on_its_pages_host_and_for_its_servers_where_thats_another() {
        let approve = |on: &str| {
            format!("Approve Roundhouse on {on}, in the browser on this Mac. Roundhouse never sees your password.")
        };

        for (page, server, on) in [
            ("auth.linear.app", "mcp.example.com", "auth.linear.app for mcp.example.com"),
            ("auth.linear.app", "mcp.example.com:8443", "auth.linear.app for mcp.example.com:8443"),
            ("mcp.example.com", "mcp.example.com", "mcp.example.com"),
            ("mcp.example.com", "MCP.Example.com", "mcp.example.com"),
            ("127.0.0.1", "127.0.0.1:54321", "127.0.0.1"),
            ("[::1]", "[::1]:8080", "[::1]"),
            ("[::1]", "[::2]", "[::1] for [::2]"),
            ("example.com", "mcp.example.com", "example.com for mcp.example.com"),
        ] {
            assert_eq!(approve_on(page, server, "Mac"), approve(on), "{page} and {server}");
        }
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
