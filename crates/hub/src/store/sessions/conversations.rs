//! Moving a chat's conversation to another device, and turning conversation mode off. Turning it on records the rule
//! across the chat, so it commits with that event, as `SessionChange::conversation` says.

use protocol::model::{ClientDevice, SessionId};
use rusqlite::params;

use super::super::{Reading, Store, StoreError};

impl Store {
    /// Moves the chat's conversation to `device`, if conversation mode is on and the conversation is on another one.
    pub fn move_conversation(&self, id: &SessionId, device: &ClientDevice) -> Result<Reading, StoreError> {
        self.change_reading(
            id,
            "UPDATE sessions SET conversation = ?1 WHERE id = ?2 AND conversation IS NOT NULL AND conversation IS NOT ?1",
            params![serde_json::to_string(device)?, id.as_str()],
        )
    }

    /// Turns the chat's conversation mode off, if it's on.
    pub fn end_conversation(&self, id: &SessionId) -> Result<Reading, StoreError> {
        self.change_reading(
            id,
            "UPDATE sessions SET conversation = NULL WHERE id = ?1 AND conversation IS NOT NULL",
            params![id.as_str()],
        )
    }
}
