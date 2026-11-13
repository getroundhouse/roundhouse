//! What reaches the worker's tools, and the hub asking the worker to serve them.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};

/// The worker's tool that waits on the workspace's pull request, by which an adapter tells a wait from the agent's
/// other work in the background.
pub const WAIT_TOOL: &str = "wait_for_pull_request";

/// Serves the worker's tools to its agents, over MCP on 127.0.0.1, on `port` if it's free and on a port of the
/// system's choosing if not, or if there's none. Replies with the port it's on, and harmless once it's serving, when it
/// replies with the port it's already on. Each agent reaches it with the token its
/// [`AgentSpec::tools_token`](super::AgentSpec::tools_token) names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServeTools {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl Request for ServeTools {
    type Reply = ToolsServing;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::ServeTools(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolsServing {
    /// The port the worker's tools are served on, on 127.0.0.1.
    pub port: u16,
}

impl Reply for ToolsServing {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::ToolsServing(serving) = response else { return Err(response) };
        Ok(serving)
    }
}

crate::model::tokens!(
    /// What an agent's calls to the worker's tools carry, as a bearer token, which tells the worker whose they are.
    /// It's only as secret as the agent's own files, since the agent's config holds it, but it's kept out of logs all
    /// the same.
    ToolsToken,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_its_text_on_the_wire_and_kept_out_of_debug_output() {
        let token = ToolsToken::new("s3cret");

        assert_eq!(serde_json::to_string(&token).unwrap(), r#""s3cret""#);
        assert!(!format!("{token:?}").contains("s3cret"));
    }
}
