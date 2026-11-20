//! How far the device that has a chat's conversation has read a reply out loud.

use serde::{Deserialize, Serialize};

use super::MessageId;

/// A place in what the agent wrote: `through` bytes into the text of the block `block` of its message `message`. A
/// block is numbered as the deltas that streamed it numbered it, which the event recording it keeps as `first_index`, so
/// the place means the same while the message streams and once it's recorded. One that never streamed is numbered by
/// its place among its event's blocks, so two such events of one message, each with a block, can't be told apart. `through` counts bytes of the block's text as the agent
/// wrote it, Markdown and all, rather than of what the voice says, and falls between two characters. It comes from
/// another build, so a reader clamps it to the text it has, and to a character's start, rather than cutting the text
/// there as given. Every block before it in the reply was said, and every one after it wasn't.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SpokenThrough {
    pub message: MessageId,
    pub block: u32,
    pub through: u32,
}

/// What cut off the voice reading a reply out.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CutBy {
    /// The person started talking over it.
    TalkedOver,
    /// The person stopped it, with Stop or by typing, or turned conversation mode off, or another device took the
    /// conversation over.
    Stopped,
    #[serde(other)]
    Unknown,
}
