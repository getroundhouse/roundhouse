//! Moving a chat's conversation to another device, and turning conversation mode off. Turning it on records the rule
//! across the chat, so it commits with that event, as `SessionChange::conversation` says. And whether a reply's voice
//! has been cut off already.

use protocol::model::{ClientDevice, MessageId, SessionId};
use rusqlite::{OptionalExtension, params};

use super::super::{Reading, Store, StoreError, lock};

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

    /// Whether the voice reading the agent's message `message` out in the chat was cut off, as a recorded
    /// [`protocol::event::EventBody::VoiceCut`] says, which happens once to a message.
    pub fn voice_cut(&self, id: &SessionId, message: &MessageId) -> Result<bool, StoreError> {
        let cut = "SELECT 1 FROM events WHERE session_id = ?1 AND kind = 'voice_cut' \
                   AND json_extract(body, '$.through.message') = ?2 LIMIT 1";
        let found: Option<i64> =
            lock(&self.reader).query_row(cut, params![id.as_str(), message.as_str()], |row| row.get(0)).optional()?;
        Ok(found.is_some())
    }
}
