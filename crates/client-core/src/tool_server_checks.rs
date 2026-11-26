//! What adding or saving a tool server comes to, once the hub has asked it for its tools, in words for a person: that
//! it's being asked, and why it listed no tools. They're of the name and the URL the person typed and of what the hub
//! says came of it, so never of anything the server said.

use protocol::client::tool_server_checks::{NoToolList, ToolServerToCheck, ToolServerUnreached};
use protocol::client::tool_server_saves::ToolServerSaved;
use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::model::{ToolServer, ToolServerUrl};

/// What's to be added or saved.
#[derive(Clone, Debug, PartialEq)]
pub enum ToSave {
    Add(ToolServerAdd),
    /// An edit, whose own `saved_anyway` is the save's to say.
    Edit(ToolServerEdit),
}

impl From<ToSave> for ToolServerToCheck {
    fn from(of: ToSave) -> Self {
        match of {
            ToSave::Add(add) => ToolServerToCheck::Add(add),
            ToSave::Edit(edit) => ToolServerToCheck::Edit(edit),
        }
    }
}

/// When it's added or saved, as the person chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Once the server has listed its tools, or at once where there's nothing to ask it.
    Listed,
    /// At once, without asking it anything, as the person chose once it didn't list them.
    Anyway,
}

/// What a save comes to.
#[derive(Clone, Debug, PartialEq)]
pub enum Saved {
    /// It was added, or the edit was saved, and this is its record.
    Saved(ToolServer),
    /// It wasn't, since it didn't list its tools, as this says, for a person, whose choice it is whether to save it
    /// anyway.
    NotListed(String),
    /// It was let go of before the hub began to save it, so nothing changed.
    LetGo,
}

/// What the hub's answer to a save comes to, for the server called `name` at `url`, as the person typed them.
pub(crate) fn saved(answer: ToolServerSaved, name: &str, url: &str) -> Saved {
    let name = name.trim();
    match answer {
        ToolServerSaved::Saved { server } => Saved::Saved(server),
        ToolServerSaved::NotListed { why } => Saved::NotListed(not_listed(why, name, &host(url))),
        ToolServerSaved::Cancelled => Saved::LetGo,
        ToolServerSaved::Unknown => {
            Saved::NotListed(format!("{name} was asked for its tools. Update this app to see what came of it."))
        }
    }
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
    use super::*;

    const URL: &str = " https://mcp.northwind.example/mcp?toolsets=all ";

    fn of(answer: ToolServerSaved) -> Saved {
        saved(answer, " Runbooks ", URL)
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
            assert_eq!(of(ToolServerSaved::NotListed { why }), Saved::NotListed(said.into()));
        }
    }

    #[test]
    fn what_a_save_comes_to_is_its_record_that_it_was_let_go_of_or_that_this_app_is_to_be_updated() {
        let server = protocol::fixtures::tool_server("runbooks");

        assert_eq!(of(ToolServerSaved::Saved { server: server.clone() }), Saved::Saved(server));
        assert_eq!(of(ToolServerSaved::Cancelled), Saved::LetGo);
        assert_eq!(
            of(ToolServerSaved::Unknown),
            Saved::NotListed("Runbooks was asked for its tools. Update this app to see what came of it.".into())
        );
    }
}
