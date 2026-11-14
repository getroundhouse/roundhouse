//! The current launch identity, with the old ID/resume pair for workers that outlived an update.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use serde_json::{Value, json};

use crate::capability::conversation::Start;
use crate::model::AgentSessionId;

pub(super) fn serialize<S: Serializer>(start: &Start, serializer: S) -> Result<S::Ok, S::Error> {
    // An old worker can still start Claude after its hub updates. Codex's chosen IDs need the new form, and an
    // old worker rejects that kind, so no placeholder conversation is sent for it.
    let wire = match start {
        Start::New { conversation: Some(id) } | Start::Resume { conversation: id } => {
            json!({"agent_session_id": id, "resume": matches!(start, Start::Resume { .. })})
        }
        Start::New { conversation: None } | Start::Fork { .. } | Start::Import { .. } => {
            serde_json::to_value(start).map_err(serde::ser::Error::custom)?
        }
    };
    wire.serialize(serializer)
}

#[derive(Deserialize)]
struct Before {
    agent_session_id: AgentSessionId,
    resume: bool,
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Start, D::Error> {
    let wire = Value::deserialize(deserializer)?;
    if wire.get("start").is_some() {
        return serde_json::from_value(wire).map_err(D::Error::custom);
    }
    let Before { agent_session_id, resume } = serde_json::from_value(wire).map_err(D::Error::custom)?;
    Ok(if resume {
        Start::Resume { conversation: agent_session_id }
    } else {
        Start::New { conversation: Some(agent_session_id) }
    })
}
