//! How far each reply said out loud was heard, as the device that read it out says.

use serde::{Deserialize, Serialize};

use super::{MessageId, Rev, SessionId};

/// A place in one of the agent's replies: `through` bytes into the text of its block `block`. A block is numbered as
/// the deltas that streamed it numbered it, which the event recording it keeps as `first_index`, so the place means the
/// same while the reply streams and once it's recorded. One that never streamed is numbered by its place among its
/// event's blocks. `through` counts bytes of the block's text as the agent wrote it, Markdown and all, rather than of
/// what the voice says, and falls between two characters. It comes from another build, so a reader clamps it to the
/// text it has, and to a character's start, rather than cutting the text there as given. Every block before it in the
/// reply was said, and every one after it wasn't. Places in one reply are in order by block, then by `through`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplyPlace {
    pub block: u32,
    pub through: u32,
}

/// How far a reply being read out has got: `through` the place in the agent's reply `message`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SpokenThrough {
    pub message: MessageId,
    pub through: ReplyPlace,
}

/// Who said how a reply ended: the device that read it out, which knows, or the hub, which settles what's left when the
/// device goes or the mode ends, from where the device last said it had got.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SettledBy {
    #[default]
    Device,
    Hub,
    /// One this build doesn't know, from a newer hub.
    #[serde(other)]
    Unknown,
}

impl SettledBy {
    pub fn is_device(&self) -> bool {
        matches!(self, SettledBy::Device)
    }
}

/// How a reply that was to be read out ended for the person hearing it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum Ending {
    /// Read out to its end.
    ToTheEnd,
    /// The person started talking over it, `through` this place.
    TalkedOver { through: ReplyPlace },
    /// Stopped `through` this place, with Stop or by typing.
    Stopped { through: ReplyPlace },
    /// Never read out at all, as when the person talked over a reply before it.
    NotReadOut,
    /// One this build doesn't know, from a newer hub.
    #[serde(other)]
    Unknown,
}

impl Ending {
    /// Where it was cut off, if it was.
    pub fn cut_at(&self) -> Option<ReplyPlace> {
        match self {
            Ending::TalkedOver { through } | Ending::Stopped { through } => Some(*through),
            Ending::ToTheEnd | Ending::NotReadOut | Ending::Unknown => None,
        }
    }
}

/// How one of a chat's replies ended for the person hearing it, as the device that had the conversation said. A reply
/// is the agent's message `message`, so one without an ID has none. Every device showing the chat draws the reply from
/// it, such as with what wasn't heard dimmed, so it's a record of the chat rather than of the device. A reply with none
/// is drawn as written, which is what a reply typed for, and one read out before the hub kept these, are.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReplyHeard {
    pub session_id: SessionId,
    pub message: MessageId,
    pub ending: Ending,
    /// Who said it. What the hub settled gives way to what the device says, which knows. Left out for the device.
    #[serde(default, skip_serializing_if = "SettledBy::is_device")]
    pub settled_by: SettledBy,
    pub rev: Rev,
}
