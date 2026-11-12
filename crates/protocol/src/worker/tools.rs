//! What reaches the worker's tools.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The worker's tool that waits on the workspace's pull request, by which an adapter tells a wait from the agent's
/// other work in the background.
pub const WAIT_TOOL: &str = "wait_for_pull_request";

/// What an agent's calls to the worker's tools carry, as a bearer token, which tells the worker whose they are. It's
/// only as secret as the agent's own files, since the agent's config holds it, but it's kept out of logs all the same.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ToolsToken(String);

impl ToolsToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ToolsToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ToolsToken(..)")
    }
}

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
