//! What the CLI says of its tool servers: `init`, which it sends as each turn starts, lists each with its status, and
//! every tool the agent has. The adapter never asks with `mcp_status`, since its answer has each server's
//! `Authorization` header in it, and the hub keeps the CLI's lines.

use protocol::capability::Report;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::event::EventBody;
use protocol::worker::TOOLS_SERVER;
use serde_json::Value;

use super::super::normalize::value_of;

/// What `body`, which the CLI said in `line`, says of the agent's tool servers, or `None` unless it's an `init` that
/// lists them.
pub(in crate::claude) fn said(body: &EventBody, line: &str) -> Option<Report> {
    let EventBody::AgentInit { .. } = body else { return None };
    let init = value_of(line)?;
    let listed: Vec<(&str, &Value)> = init["mcp_servers"]
        .as_array()?
        .iter()
        .filter_map(|server| Some((server["name"].as_str()?, &server["status"])))
        .collect();
    let tools = ToolCounts::of(&init["tools"], listed.iter().map(|(name, _)| *name));
    let the_agents = listed.iter().filter(|(name, _)| *name != TOOLS_SERVER);
    let servers = the_agents.map(|(name, status)| {
        let state = state(status);
        ToolServerState {
            name: (*name).into(),
            hubs: None,
            state,
            // `init` never says why one failed.
            error: None,
            tools: if state == ServerState::Connected { tools.of_server(name) } else { None },
        }
    });
    Some(Report::ToolServers { servers: servers.collect() })
}

fn state(status: &Value) -> ServerState {
    match status.as_str() {
        Some("connected") => ServerState::Connected,
        Some("needs-auth") => ServerState::NeedsSignIn,
        Some("failed") => ServerState::Failed,
        Some("pending") => ServerState::Starting,
        Some("disabled") => ServerState::Disabled,
        _ => ServerState::Unknown,
    }
}

/// How many tools the agent has from each server, by what starts their names.
struct ToolCounts {
    /// Each server's prefix, and how many tools have it and no longer one.
    by_prefix: Vec<(String, u32)>,
}

impl ToolCounts {
    /// Of `tools`, `init`'s list of names, for the servers `names`. A tool is the server's whose prefix is the longest
    /// its name starts with, since a server `docs__old` has tools whose names start with the server `docs`'s prefix too.
    fn of<'a>(tools: &Value, names: impl Iterator<Item = &'a str>) -> Self {
        let mut by_prefix: Vec<(String, u32)> = names.map(|name| (prefix(name), 0)).collect();
        for tool in tools.as_array().into_iter().flatten().filter_map(Value::as_str) {
            let with = by_prefix.iter_mut().filter(|(prefix, _)| tool.starts_with(prefix.as_str()));
            if let Some((_, count)) = with.max_by_key(|(prefix, _)| prefix.len()) {
                *count += 1;
            }
        }
        Self { by_prefix }
    }

    /// How many tools the server `name` gives, or `None` if another server's tools have names that start the same way.
    fn of_server(&self, name: &str) -> Option<u32> {
        let prefix = prefix(name);
        let mut of_it = self.by_prefix.iter().filter(|(with, _)| *with == prefix);
        match (of_it.next(), of_it.next()) {
            (Some((_, count)), None) => Some(*count),
            _ => None,
        }
    }
}

/// What the CLI starts the name of each of the server `name`'s tools with. The names of claude.ai's connectors have
/// each run of `_` made one, and none at either end.
pub(super) fn prefix(name: &str) -> String {
    let mut spelled = spelled(name);
    if name.starts_with("claude.ai ") {
        spelled = spelled.split('_').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("_");
    }
    format!("mcp__{spelled}__")
}

/// `name`, a server's or a tool's, as the CLI has it in a tool's name: anything but a letter, digit, `-` or `_` of
/// ASCII is a `_`, one for each of its UTF-16 units, since the CLI's regular expression reads those.
pub(super) fn spelled(name: &str) -> String {
    let mut spelled = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
            spelled.push(c);
        } else {
            spelled.extend(std::iter::repeat_n('_', c.len_utf16()));
        }
    }
    spelled
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::claude::normalize::{Normalizer, Parsed};

    /// What the adapter reports of the line `init`.
    fn reported(init: &Value) -> Option<Vec<ToolServerState>> {
        let line = init.to_string();
        let Parsed::Event(body) = Normalizer::default().line(&line).parsed else { panic!("not an event") };
        let Report::ToolServers { servers } = said(&body, &line)? else { panic!("another report") };
        Some(servers)
    }

    fn init(servers: &Value, tools: &[&str]) -> Value {
        json!({"type": "system", "subtype": "init", "session_id": "c", "tools": tools, "mcp_servers": servers})
    }

    fn server(name: &str, status: &str) -> Value {
        json!({"name": name, "status": status, "source": "user"})
    }

    fn is(name: &str, state: ServerState, tools: Option<u32>) -> ToolServerState {
        ToolServerState { name: name.into(), hubs: None, state, error: None, tools }
    }

    #[test]
    fn each_server_init_lists_has_its_status_and_how_many_tools_a_connected_one_gives() {
        let servers = json!([
            server("roundhouse", "connected"),
            server("Docs Site.v2", "connected"),
            server("empty", "connected"),
            server("tickets", "needs-auth"),
            server("gone", "failed"),
            server("slow", "pending"),
            server("off", "disabled"),
            server("later", "resting"),
        ]);
        let tools = ["Bash", "mcp__Docs_Site_v2__find", "mcp__Docs_Site_v2__read", "mcp__roundhouse__propose"];

        assert_eq!(
            reported(&init(&servers, &tools)),
            Some(vec![
                is("Docs Site.v2", ServerState::Connected, Some(2)),
                is("empty", ServerState::Connected, Some(0)),
                is("tickets", ServerState::NeedsSignIn, None),
                is("gone", ServerState::Failed, None),
                is("slow", ServerState::Starting, None),
                is("off", ServerState::Disabled, None),
                is("later", ServerState::Unknown, None),
            ]),
            "Roundhouse's own is left out"
        );
    }

    #[test]
    fn a_tool_counts_for_the_one_server_whose_name_its_own_starts_with() {
        let connected = |names: &[&str]| json!(names.iter().map(|name| server(name, "connected")).collect::<Vec<_>>());
        let counts = |names: &[&str], tools: &[&str]| -> Vec<Option<u32>> {
            reported(&init(&connected(names), tools)).unwrap().into_iter().map(|server| server.tools).collect()
        };

        assert_eq!(counts(&["a", "a__b"], &["mcp__a__b__x", "mcp__a__y", "mcp__a__z"]), [Some(2), Some(1)]);
        assert_eq!(counts(&["a.b", "a b"], &["mcp__a_b__x"]), [None, None], "either's");
        assert_eq!(counts(&["claude.ai  Mail (Work)"], &["mcp__claude_ai_Mail_Work__x"]), [Some(1)]);
        assert_eq!(counts(&["docs \u{1f5c2}"], &["mcp__docs_____x"]), [Some(1)], "an emoji is two UTF-16 units");
    }

    /// So a chat with none says so, and one whose CLI doesn't say is not known.
    #[test]
    fn an_init_that_lists_none_says_there_are_none_and_one_with_no_list_says_nothing() {
        assert_eq!(reported(&init(&json!([]), &[])), Some(Vec::new()));
        assert_eq!(reported(&init(&json!([server("roundhouse", "connected")]), &[])), Some(Vec::new()));
        assert_eq!(reported(&json!({"type": "system", "subtype": "init", "session_id": "c"})), None);
    }
}
