//! What the CLI says of its tool servers: `init`, which it sends as each turn starts, lists each with its status, and
//! every tool the agent has, and what it answers of one it's asked to connect to again says how that one is until the
//! next. The adapter never asks with `mcp_status`, since its answer has each server's
//! `Authorization` header in it, and the hub keeps the CLI's lines.

use std::collections::HashMap;

use protocol::capability::Report;
use protocol::capability::setup::{ServerName, ServerState, ToolServerState};
use protocol::event::EventBody;
use protocol::worker::TOOLS_SERVER;
use serde_json::Value;

use super::super::normalize::value_of;

/// What the CLI last said of its tool servers, whole, with what it has answered of one since, and which of them it's been
/// asked to connect to again, by the request that asked.
#[derive(Default)]
pub(in crate::claude) struct ToolServers {
    last: Vec<ToolServerState>,
    reconnecting: HashMap<String, ServerName>,
}

impl ToolServers {
    /// Takes `servers`, as an `init` lists them, and returns them to report.
    pub(in crate::claude) fn listed(&mut self, servers: Vec<ToolServerState>) -> Report {
        self.last.clone_from(&servers);
        Report::ToolServers { servers }
    }

    /// The CLI is being asked to connect to `server` again, in the request `request_id`. Returns the servers to report,
    /// with that one starting, where the CLI isn't connected to it.
    pub(in crate::claude) fn reconnecting(&mut self, request_id: String, server: ServerName) -> Option<Report> {
        let starting = matches!(self.state_of(&server), Some(ServerState::Failed | ServerState::NeedsSignIn));
        let report = if starting { self.came_to(server.clone(), ServerState::Starting) } else { None };
        self.reconnecting.insert(request_id, server);
        report
    }

    /// The request `request_id` wasn't written, so nothing answers it, and the CLI has gone.
    pub(in crate::claude) fn unasked(&mut self, request_id: &str) {
        self.reconnecting.remove(request_id);
    }

    /// Takes the CLI's answer to the request `request_id`, if it's one to connect to a server again: that it
    /// `connected`, or that it failed. Returns the servers to report, whole. How many tools the server gives isn't known
    /// until the next `init` lists them, and why it failed isn't kept, since no `init` says it, and the CLI's words for it
    /// can hold what a server said.
    pub(in crate::claude) fn answered(&mut self, request_id: &str, connected: bool) -> Option<Report> {
        let name = self.reconnecting.remove(request_id)?;
        self.came_to(name, if connected { ServerState::Connected } else { ServerState::Failed })
    }

    fn state_of(&self, name: &ServerName) -> Option<ServerState> {
        self.last.iter().find(|server| server.name == *name).map(|server| server.state)
    }

    /// The servers with `name` as `state`, or `None` for one the CLI never listed, which it doesn't have.
    fn came_to(&mut self, name: ServerName, state: ServerState) -> Option<Report> {
        let server = self.last.iter_mut().find(|server| server.name == name)?;
        if server.state != state {
            *server = ToolServerState { name, hubs: None, state, error: None, tools: None };
        }
        Some(Report::ToolServers { servers: self.last.clone() })
    }
}

/// What `body`, which the CLI said in `line`, says of the agent's tool servers, or `None` unless it's an `init` that
/// lists them.
pub(in crate::claude) fn said(body: &EventBody, line: &str) -> Option<Vec<ToolServerState>> {
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
    Some(servers.collect())
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
        said(&body, &line)
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

    /// What's reported of having the CLI connect to `server` again, as it's asked and as it answers, after an `init` that
    /// listed two.
    fn asked_for(server: &str, connected: bool) -> [Option<Vec<ToolServerState>>; 2] {
        let mut servers = ToolServers::default();
        servers.listed(vec![is("docs", ServerState::Connected, Some(2)), is("linear", ServerState::Failed, None)]);
        let asked = servers.reconnecting("r".into(), server.into());
        assert_eq!(servers.answered("another", connected), None, "an answer to something else");
        let answered = servers.answered("r", connected);
        assert_eq!(servers.answered("r", connected), None, "only once");
        [asked, answered].map(|report| {
            let Report::ToolServers { servers } = report? else { panic!("another report") };
            Some(servers)
        })
    }

    #[test]
    fn a_server_connected_to_again_is_starting_and_then_what_the_cli_answers_until_the_next_init() {
        let linear = |state| Some(vec![is("docs", ServerState::Connected, Some(2)), is("linear", state, None)]);

        assert_eq!(asked_for("linear", true), [linear(ServerState::Starting), linear(ServerState::Connected)]);
        assert_eq!(asked_for("linear", false), [linear(ServerState::Starting), linear(ServerState::Failed)]);
        assert_eq!(asked_for("docs", true), [None, linear(ServerState::Failed)], "it was connected, with its tools");
        assert_eq!(asked_for("nothing", false), [None, None], "a server it doesn't have");
    }

    /// So a chat with none says so, and one whose CLI doesn't say is not known.
    #[test]
    fn an_init_that_lists_none_says_there_are_none_and_one_with_no_list_says_nothing() {
        assert_eq!(reported(&init(&json!([]), &[])), Some(Vec::new()));
        assert_eq!(reported(&init(&json!([server("roundhouse", "connected")]), &[])), Some(Vec::new()));
        assert_eq!(reported(&json!({"type": "system", "subtype": "init", "session_id": "c"})), None);
    }
}
