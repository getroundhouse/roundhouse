use std::io;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServerAuth, ToolServerAuthGiven, ToolServerHeader, ToolServerKey};
use serde::{Deserialize, Serialize};

use crate::login_files::{LoginSecret, Stored};

/// What a tool server knows the person by, in a file of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ToolServerSecret {
    /// A key the person gave a client.
    Key(ToolServerKey),
}

/// How a file holds what it holds: a JSON object whose `format` says which form it's in. A form only a newer build knows
/// is kept for that build, as [`Stored::Newer`] says, so going back to an older build loses nothing. So a change that
/// only adds a field keeps the form, and a change an older build would misread is a new form.
#[derive(Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
enum Form {
    Key {
        key: ToolServerKey,
    },
    #[serde(other)]
    Newer,
}

impl LoginSecret for ToolServerSecret {
    const DIR: &'static str = protocol::paths::TOOL_SERVERS_DIR;

    fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let ToolServerSecret::Key(key) = self;
        Ok(serde_json::to_vec(&Form::Key { key: key.clone() })?)
    }

    /// What isn't one of the forms, such as one cut short, holds none.
    fn from_bytes(bytes: Vec<u8>) -> Stored<Self> {
        match serde_json::from_slice(&bytes) {
            Ok(Form::Key { key }) => Stored::Secret(ToolServerSecret::Key(key)),
            Ok(Form::Newer) => Stored::Newer,
            Err(_) => Stored::Nothing,
        }
    }
}

/// Longer than any key a server gives.
const MAX_KEY_BYTES: usize = 4096;

/// How a server being added is to know the person, for its record, and what it's to know them by, for its file, as a
/// client `given` both. The key is trimmed, since what's pasted often ends in a line break. Refused with
/// `InvalidRequest` when the header can't be one, and when the key is empty, very long, or can't be sent in a header,
/// as one with a line break inside can't, which could end the header and start another. A key can have a space inside,
/// as one with its scheme, such as `Bearer`, has.
pub(super) fn read(given: ToolServerAuthGiven) -> Result<(ToolServerAuth, Option<ToolServerSecret>), ApiError> {
    let refused = |message: &str| Err(ApiError::new(ErrorCode::InvalidRequest, message));
    match given {
        ToolServerAuthGiven::Key { header, key } => {
            let header = ToolServerHeader::typed(&header)?;
            let key = key.as_str().trim();
            if key.is_empty() || key.len() > MAX_KEY_BYTES {
                return refused("the key is empty, or longer than any key is");
            }
            if !key.chars().all(|c| c == ' ' || c.is_ascii_graphic()) {
                return refused("the key has a line break inside, or another character that can't be sent in a header");
            }
            let secret = ToolServerSecret::Key(ToolServerKey::new(key));
            Ok((ToolServerAuth::Key { header }, Some(secret)))
        }
        ToolServerAuthGiven::Neither => Ok((ToolServerAuth::Neither, None)),
        // Not `Unsupported`, which says the hub has no tool servers of its own at all.
        ToolServerAuthGiven::Unknown => refused("this hub doesn't know that way for an MCP server to know who you are"),
    }
}
