//! What a launch of protocol 3 starts from: a chosen or resumed conversation as its ID and whether to resume it, as a
//! worker from before `Start` reads it, and anything else in `Start`'s own form.

use serde::{Serialize, Serializer};
use serde_json::json;

use crate::capability::conversation::Start;

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
