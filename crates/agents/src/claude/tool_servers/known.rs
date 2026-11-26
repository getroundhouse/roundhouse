use protocol::event::TaskTool;
use serde_json::Value;

use super::said::{prefix, spelled};
use crate::ToolServers;

/// The tool servers the CLI has, and the tools, as its newest `init` lists them. Before one, the servers it was launched
/// with.
#[derive(Debug, Default)]
pub(in crate::claude) struct Servers {
    names: Vec<String>,
    tools: Vec<String>,
}

impl Servers {
    pub(in crate::claude) fn launched_with(servers: Option<&ToolServers>) -> Self {
        let names = servers.map(|servers| servers.named().map(|(name, _)| name.to_owned()).collect());
        Self { names: names.unwrap_or_default(), tools: Vec::new() }
    }

    /// Notes what this line lists, if it's an `init` that lists any.
    pub(in crate::claude) fn note(&mut self, value: &Value) {
        if value["type"] != "system" || value["subtype"] != "init" {
            return;
        }
        if let Some(servers) = value["mcp_servers"].as_array() {
            self.names = servers.iter().filter_map(|server| server["name"].as_str()).map(str::to_owned).collect();
        }
        if let Some(tools) = value["tools"].as_array() {
            self.tools = tools.iter().filter_map(Value::as_str).map(str::to_owned).collect();
        }
    }

    /// The tool whose task the CLI describes as `description`, which is the server's name, a `/` and the tool's. Either
    /// name can have a `/` of its own, so where there are several, it's the one after a server there is, and where two
    /// servers fit, the one whose tool there is. `None` where that doesn't say which, rather than a guess.
    pub(in crate::claude) fn tool_of(&self, description: &str) -> Option<TaskTool> {
        let halves = description.match_indices('/').map(|(at, _)| (&description[..at], &description[at + 1..]));
        let halves: Vec<(&str, &str)> =
            halves.filter(|(server, tool)| !server.is_empty() && !tool.is_empty()).collect();
        let (server, tool) = only(&halves, |_| true)
            .or_else(|| only(&halves, |(server, _)| self.has(server)))
            .or_else(|| only(&halves, |(server, tool)| self.has(server) && self.has_tool(server, tool)))?;
        Some(TaskTool { server: server.into(), tool: tool.to_owned() })
    }

    fn has(&self, server: &str) -> bool {
        self.names.iter().any(|name| name == server)
    }

    fn has_tool(&self, server: &str, tool: &str) -> bool {
        let name = format!("{}{}", prefix(server), spelled(tool));
        self.tools.contains(&name)
    }
}

/// The one of `halves` that `fits`, if only one does.
fn only<'a>(halves: &[(&'a str, &'a str)], fits: impl Fn(&(&'a str, &'a str)) -> bool) -> Option<(&'a str, &'a str)> {
    let mut fitting = halves.iter().filter(|half| fits(half));
    match (fitting.next(), fitting.next()) {
        (Some(one), None) => Some(*one),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn read(servers: &Servers, description: &str) -> Option<(String, String)> {
        servers.tool_of(description).map(|task| (task.server.to_string(), task.tool))
    }

    fn halves(server: &str, tool: &str) -> (String, String) {
        (server.into(), tool.into())
    }

    #[test]
    fn a_task_is_of_the_server_there_is_whichever_name_has_a_slash() {
        let mut servers = Servers::default();
        assert_eq!(read(&servers, "docs/find"), Some(halves("docs", "find")), "one slash says which");
        assert_eq!(read(&servers, "team/docs/find"), None, "either's, with no server known");

        let listed = ["docs", "team/docs", "team"].map(|name| json!({"name": name, "status": "connected"}));
        let tools = ["Bash", "mcp__docs__pages_find", "mcp__team_docs__find", "mcp__team__list"];
        servers.note(&json!({"type": "system", "subtype": "init", "mcp_servers": listed, "tools": tools}));

        assert_eq!(read(&servers, "docs/pages/find"), Some(halves("docs", "pages/find")), "the tool's");
        assert_eq!(
            read(&servers, "team/docs/find"),
            Some(halves("team/docs", "find")),
            "the server's, as the tools say"
        );
        assert_eq!(read(&servers, "team/docs/gone"), None, "neither server has it");
        assert_eq!(read(&servers, "other/pages/find"), None, "of no server there is");
        for nothing in ["find", "/find", "docs/", "/", ""] {
            assert_eq!(read(&servers, nothing), None, "{nothing:?}");
        }
    }
}
