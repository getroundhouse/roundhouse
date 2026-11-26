//! What checking a tool server comes to, as it's to be added or edited, in words for a person: that it's being asked,
//! and why it listed no tools. They're of the name and the URL the person typed and of what the hub says came of it,
//! so never of anything the server said.

use protocol::client::tool_server_checks::{NoToolList, ToolServerChecked, ToolServerUnreached};
use protocol::model::ToolServerUrl;

use crate::Result;

/// What a check comes to, for a client that then adds or edits the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Checked {
    /// There's nothing to say before it's added or edited: it listed its tools, there was nothing to ask, or the hub is
    /// too old to check one.
    Fine,
    /// It didn't list them, as this says, for a person, whose choice it is whether to go on anyway.
    Not(String),
}

/// What `answer` comes to, for the server called `name` at `url`, as the person typed them. An error is one to refuse
/// what's checked with.
pub fn came_to(answer: Result<ToolServerChecked>, name: &str, url: &str) -> Result<Checked> {
    let checked = match answer {
        Err(e) if e.is_unsupported() => return Ok(Checked::Fine),
        answer => answer?,
    };
    let name = name.trim();
    Ok(match checked {
        ToolServerChecked::NothingToAsk | ToolServerChecked::Listed { tools: _ } => Checked::Fine,
        ToolServerChecked::NotListed { why } => Checked::Not(not_listed(why, name, &host(url))),
        ToolServerChecked::Unknown => {
            Checked::Not(format!("{name} was asked for its tools. Update this app to see what came of it."))
        }
    })
}

/// That the server at `url`, as the person typed it, is being asked.
pub fn asking(url: &str) -> String {
    format!("Asking {} for its tools", host(url))
}

/// Where the server at `url` is. One that isn't a URL is refused before it's asked.
fn host(url: &str) -> String {
    ToolServerUrl::typed(url).map_or_else(|_| "the server".to_string(), |url| url.host().to_string())
}

fn not_listed(why: NoToolList, name: &str, host: &str) -> String {
    match why {
        NoToolList::Unreached { why } => match why.and_then(unreached) {
            Some(why) => format!("The hub can't reach {host}. {why}"),
            None => format!("The hub can't reach {host}."),
        },
        NoToolList::TimedOut => format!("{host} didn't answer in time."),
        NoToolList::Lost => format!("The hub lost its connection to {host} before it answered."),
        NoToolList::Redirected => {
            format!("{host} sends the hub on to another URL. Type that one if it's this server's.")
        }
        NoToolList::KeyRefused { status: _ } => format!("{name} refused this key."),
        NoToolList::WantsSignInOrKey => format!("{name} asks who you are, so it needs a sign-in or a key."),
        NoToolList::Forbidden => format!("{name} doesn't let the hub in (403). It may need a sign-in or a key."),
        NoToolList::NotFound => format!("{host} has nothing at this URL (404)."),
        NoToolList::TooManyRequests => format!("{host} has had too many requests for now (429)."),
        NoToolList::Failing { status } => format!("{host} has a problem of its own ({status})."),
        NoToolList::NotMcp => format!("{host} isn't an MCP server at this URL."),
        NoToolList::TooLong => format!("{host} answered with more than the hub reads."),
        NoToolList::WontConnect => format!("{name} answered, and wouldn't connect."),
        NoToolList::WontList => format!("{name} connected, and wouldn't list its tools."),
        NoToolList::Unknown => format!("{name} didn't list its tools. Update this app to see why."),
    }
}

fn unreached(why: ToolServerUnreached) -> Option<&'static str> {
    match why {
        ToolServerUnreached::NoSuchName => Some("There's no such host."),
        ToolServerUnreached::ConnectionRefused => Some("Connection refused."),
        ToolServerUnreached::NoRoute => Some("There's no way to it from the hub's machine."),
        ToolServerUnreached::NotTrusted => Some("The hub's machine doesn't trust its certificate."),
        ToolServerUnreached::NotHttps => Some("It doesn't speak https there."),
        ToolServerUnreached::NoCertificates => Some("The hub's machine doesn't say which certificates it trusts."),
        ToolServerUnreached::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use protocol::client::{ApiError, ErrorCode};

    use super::*;
    use crate::ClientError;

    const URL: &str = " https://mcp.northwind.example/mcp?toolsets=all ";

    fn of(answer: Result<ToolServerChecked>) -> Checked {
        came_to(answer, " Runbooks ", URL).unwrap()
    }

    #[test]
    fn a_server_that_lists_its_tools_or_isnt_asked_for_them_is_gone_on_with_and_so_is_one_an_old_hub_cant_check() {
        for checked in [ToolServerChecked::Listed { tools: 0 }, ToolServerChecked::NothingToAsk] {
            assert_eq!(of(Ok(checked)), Checked::Fine);
        }
        let refused = |code| Err(ClientError::Api(ApiError::new(code, "refused")));

        assert_eq!(of(refused(ErrorCode::Unsupported)), Checked::Fine);
        let taken = came_to(refused(ErrorCode::Conflict), "Runbooks", URL);
        assert!(matches!(taken, Err(e) if e.is_conflict()), "as adding it would be");
    }

    #[test]
    fn a_server_thats_asked_is_said_by_its_host() {
        assert_eq!(asking(URL), "Asking mcp.northwind.example for its tools");
        assert_eq!(asking("http://127.0.0.1:8080/mcp"), "Asking 127.0.0.1:8080 for its tools");
        assert_eq!(asking("https://ada:secret@mcp.northwind.example"), "Asking the server for its tools");
    }

    #[test]
    fn why_a_server_listed_no_tools_is_said_by_the_name_and_the_host_that_were_typed() {
        let unreached = |why| NoToolList::Unreached { why: Some(why) };
        for (why, said) in [
            (NoToolList::KeyRefused { status: 401 }, "Runbooks refused this key."),
            (NoToolList::KeyRefused { status: 403 }, "Runbooks refused this key."),
            (NoToolList::WantsSignInOrKey, "Runbooks asks who you are, so it needs a sign-in or a key."),
            (NoToolList::Unreached { why: None }, "The hub can't reach mcp.northwind.example."),
            (
                unreached(ToolServerUnreached::ConnectionRefused),
                "The hub can't reach mcp.northwind.example. Connection refused.",
            ),
            (NoToolList::NotMcp, "mcp.northwind.example isn't an MCP server at this URL."),
            (NoToolList::Failing { status: 503 }, "mcp.northwind.example has a problem of its own (503)."),
            (NoToolList::WontList, "Runbooks connected, and wouldn't list its tools."),
            (
                unreached(ToolServerUnreached::NoSuchName),
                "The hub can't reach mcp.northwind.example. There's no such host.",
            ),
            (
                unreached(ToolServerUnreached::NoRoute),
                "The hub can't reach mcp.northwind.example. There's no way to it from the hub's machine.",
            ),
            (
                unreached(ToolServerUnreached::NotTrusted),
                "The hub can't reach mcp.northwind.example. The hub's machine doesn't trust its certificate.",
            ),
            (
                unreached(ToolServerUnreached::NotHttps),
                "The hub can't reach mcp.northwind.example. It doesn't speak https there.",
            ),
            (
                unreached(ToolServerUnreached::NoCertificates),
                "The hub can't reach mcp.northwind.example. The hub's machine doesn't say which certificates it \
                 trusts.",
            ),
            (unreached(ToolServerUnreached::Unknown), "The hub can't reach mcp.northwind.example."),
            (NoToolList::TimedOut, "mcp.northwind.example didn't answer in time."),
            (NoToolList::Lost, "The hub lost its connection to mcp.northwind.example before it answered."),
            (
                NoToolList::Redirected,
                "mcp.northwind.example sends the hub on to another URL. Type that one if it's this server's.",
            ),
            (NoToolList::Forbidden, "Runbooks doesn't let the hub in (403). It may need a sign-in or a key."),
            (NoToolList::NotFound, "mcp.northwind.example has nothing at this URL (404)."),
            (NoToolList::TooManyRequests, "mcp.northwind.example has had too many requests for now (429)."),
            (NoToolList::TooLong, "mcp.northwind.example answered with more than the hub reads."),
            (NoToolList::WontConnect, "Runbooks answered, and wouldn't connect."),
            (NoToolList::Unknown, "Runbooks didn't list its tools. Update this app to see why."),
        ] {
            assert_eq!(of(Ok(ToolServerChecked::NotListed { why })), Checked::Not(said.into()));
        }
        assert_eq!(
            of(Ok(ToolServerChecked::Unknown)),
            Checked::Not("Runbooks was asked for its tools. Update this app to see what came of it.".into())
        );
    }
}
