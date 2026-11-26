use std::io;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServerAuth, ToolServerAuthGiven, ToolServerHeader, ToolServerKey};
use serde::{Deserialize, Serialize};

use super::kept::KeptSignIn;
use crate::login_files::{LoginSecret, Stored};

/// What a tool server knows the person by, in a file of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ToolServerSecret {
    /// A key the person gave a client.
    Key(ToolServerKey),
    SignIn(Box<KeptSignIn>),
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
    SignIn(Box<KeptSignIn>),
    #[serde(other)]
    Newer,
}

impl LoginSecret for ToolServerSecret {
    const DIR: &'static str = protocol::paths::TOOL_SERVERS_DIR;

    fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let form = match self {
            ToolServerSecret::Key(key) => Form::Key { key: key.clone() },
            ToolServerSecret::SignIn(kept) => Form::SignIn(kept.clone()),
        };
        Ok(serde_json::to_vec(&form)?)
    }

    /// What isn't one of the forms, such as one cut short, holds none.
    fn from_bytes(bytes: Vec<u8>) -> Stored<Self> {
        match serde_json::from_slice(&bytes) {
            Ok(Form::Key { key }) => Stored::Secret(ToolServerSecret::Key(key)),
            Ok(Form::SignIn(kept)) => Stored::Secret(ToolServerSecret::SignIn(kept)),
            Ok(Form::Newer) => Stored::Newer,
            Err(_) => Stored::Nothing,
        }
    }
}

/// Longer than any key a server gives.
const MAX_KEY_BYTES: usize = 4096;

/// How a server being added is to know the person, for its record, and what it's to know them by, for its file, as a
/// client `given` both. Refused with `InvalidRequest` when the header can't be one, and as [`read_key`] says.
pub(super) fn read(given: ToolServerAuthGiven) -> Result<(ToolServerAuth, Option<ToolServerSecret>), ApiError> {
    match given {
        ToolServerAuthGiven::Key { header, key } => {
            let header = ToolServerHeader::typed(&header)?;
            let secret = ToolServerSecret::Key(read_key(&key)?);
            Ok((ToolServerAuth::Key { header }, Some(secret)))
        }
        ToolServerAuthGiven::Neither => Ok((ToolServerAuth::Neither, None)),
        ToolServerAuthGiven::Unknown => Err(unknown_way()),
    }
}

/// A key as it's kept, which is as a client gave it, trimmed, since what's pasted often ends in a line break. Refused
/// with `InvalidRequest` when it's empty, very long, or can't be sent in a header, as one with a line break inside
/// can't, which could end the header and start another. A key can have a space inside, as one with its scheme, such as
/// `Bearer`, has.
pub(super) fn read_key(given: &ToolServerKey) -> Result<ToolServerKey, ApiError> {
    let key = given.as_str().trim();
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(refused("the key is empty, or longer than any key is"));
    }
    if !key.chars().all(|c| c == ' ' || c.is_ascii_graphic()) {
        return Err(refused("the key has a line break inside, or another character that can't be sent in a header"));
    }
    Ok(ToolServerKey::new(key))
}

/// Why a way only a newer client knows is refused. Not `Unsupported`, which says the hub has no tool servers of its own
/// at all.
pub(super) fn unknown_way() -> ApiError {
    refused("this hub doesn't know that way for an MCP server to know who you are")
}

pub(super) fn refused(message: &str) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, message)
}

/// What `header` holds for `key`, which is kept as it was given, so it's right in another header too. Servers give out
/// a key alone, to be sent as a bearer token, so one in `Authorization` with no space in it is one without its scheme.
/// Any other header holds what its server says it does, so a key in one is sent as it is.
pub(super) fn as_sent(header: &ToolServerHeader, key: &ToolServerKey) -> String {
    let key = key.as_str();
    if header.is_authorization() && !key.contains(' ') { format!("Bearer {key}") } else { key.to_owned() }
}
