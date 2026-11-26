#![cfg(test)]
use protocol::capability::Control;
use serde_json::json;

use super::harness::Harness;
use crate::{AgentError, AgentProcess};

/// `agy` tries a failed server again by itself at each prompt, so it claims it can't be asked to, and isn't.
#[tokio::test]
async fn a_server_is_never_asked_to_be_connected_to_again() {
    let mut h = Harness::on_model(&json!([]), "gemini-4-carbon");
    let agent = h.started().await;

    let setup = agent.capabilities().setup;
    assert_eq!((setup.tool_server_reconnect, setup.tool_server_reload), (None, None));
    for control in [Control::Reconnect { server: "docs".into() }, Control::Reload { server: "docs".into() }] {
        let refused = agent.control(&control).await;
        assert!(matches!(refused, Err(AgentError::UnknownRequest(_))), "{refused:?}");
    }

    agent.stop().await;
}
