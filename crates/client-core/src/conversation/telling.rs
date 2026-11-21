//! What reading out has the voice do and tells the hub, and what the voice tells it back, as [`super::Reading`]
//! says them.

use protocol::model::{Ending, MessageId, SpokenThrough};

/// A text given to the voice, as reading out names it: a block of a reply, or a cover line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceText(pub u64);

/// What to have the voice do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToVoice {
    /// More of `text`, which starts `at` bytes into it, as the voice's own `say` takes it.
    Say { text: VoiceText, at: usize, more: String },
    /// All of `text` has been given.
    End(VoiceText),
    /// Drop whatever's still to play.
    Stop,
}

/// What the voice said, as reading out takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromVoice {
    /// Everything in `text` up to `through` has been heard.
    Heard { text: VoiceText, through: usize },
    /// All of `text` has been heard.
    Finished(VoiceText),
    /// It stopped, as asked or after a failure: `cut` is the text being read and how far it had been heard, and
    /// `never_started` the texts after it.
    Stopped { cut: Option<(VoiceText, usize)>, never_started: Vec<VoiceText> },
    /// It can't read `text` at all, as while its files are still being fetched.
    Unread(VoiceText),
}

/// What to tell the hub, in the order given, since how a reply ended has to land before the mode is turned off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToHub {
    /// How far the reply being read has got, which every device showing the chat follows.
    Speaking(SpokenThrough),
    /// How `message` ended for the person hearing it.
    Heard { message: MessageId, ending: Ending },
}

/// Something to do, for the voice or the hub, in the order reading out says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    Voice(ToVoice),
    Hub(ToHub),
}
