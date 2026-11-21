//! Where the conversation stands on this device, as the bar that takes the composer's place says it, the same on
//! every client, which each draws its own way, such as with a key to hold on a Mac and a button on a phone.

use protocol::model::{DeviceId, Session};
use protocol::tail::Tail;

/// What the conversation is at, on the device that has it, which every device showing the chat shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for the person to talk.
    Waiting,
    /// Hearing the person, while they hold to talk. What's heard so far is the hearing's own.
    Hearing,
    /// The agent is at work, with what it's doing, said for it while it can't answer, once there's a line.
    Working { line: Option<String> },
    /// The voice is reading a reply, or a line said for the agent.
    Speaking,
}

/// Whether this device's voice can read.
#[derive(Clone, Debug, PartialEq)]
pub enum VoiceReady {
    Ready,
    /// Its files are being fetched, this far, from 0 to 1, when that's known.
    Fetching(Option<f32>),
    /// It can't read, for this reason, which a person can try again.
    Failed(String),
}

/// What the bar shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Shows {
    /// The mode is off, so the composer shows.
    Composer,
    /// The conversation is on this device.
    Here { stage: Stage, voice: VoiceReady },
    /// It's on another device, named so, which this one can take it over from, at `stage` there when the chat says,
    /// as while the agent works or the other device reads.
    Elsewhere { device: String, stage: Option<Stage> },
    /// It was on this device, and another, named so, took it over, which this one can take it back from.
    MovedTo { device: String },
}

/// What this device knows that the chat doesn't: whether it hears or reads, and whether its voice is ready.
#[derive(Clone, Debug, PartialEq)]
pub struct HereNow {
    /// The person is holding to talk.
    pub hearing: bool,
    /// The voice is reading a reply. While it reads only a line said for the agent, the bar shows the line, as every
    /// device does.
    pub reading: bool,
    pub voice: VoiceReady,
}

/// The bar of one chat on this device. It remembers whether the conversation was here, so that once another device
/// takes it over, the bar says where it went rather than only where it is.
#[derive(Debug, Default)]
pub struct Bar {
    had_it: bool,
}

impl Bar {
    /// Takes in the chat's record, `session`, as it changes, on the device `this`, or on none, as the command line is:
    /// whether the conversation is here, and whether the mode has ended, so it's no longer anywhere.
    pub fn saw(&mut self, session: &Session, this: Option<&DeviceId>) {
        match session.interaction.device() {
            Some(device) if Some(&device.id) == this => self.had_it = true,
            Some(_) => {}
            None => self.had_it = false,
        }
    }

    /// What the bar shows for `session`, whose tail is `tail`, on the device `this`, once it has [`Self::saw`] the
    /// record.
    pub fn shows(&self, session: &Session, tail: &Tail, this: Option<&DeviceId>, here: &HereNow) -> Shows {
        let Some(device) = session.interaction.device() else {
            return Shows::Composer;
        };
        let working = session
            .status
            .in_turn()
            .then(|| Stage::Working { line: tail.covering.as_deref().map(|covering| covering.line.text.clone()) });
        if Some(&device.id) == this {
            let stage = if here.hearing {
                Stage::Hearing
            } else if here.reading {
                Stage::Speaking
            } else {
                working.unwrap_or(Stage::Waiting)
            };
            return Shows::Here { stage, voice: here.voice.clone() };
        }
        if self.had_it {
            return Shows::MovedTo { device: device.name.clone() };
        }
        let stage = if tail.speaking.is_some() { Some(Stage::Speaking) } else { working };
        Shows::Elsewhere { device: device.name.clone(), stage }
    }
}

impl Shows {
    /// What the bar says, in the words every client uses, or `None` where each says it its own way: how to talk, as
    /// with a key held on a Mac and a button on a phone, and what's being heard, which is the hearing's own.
    pub fn says(&self) -> Option<String> {
        match self {
            Shows::Composer | Shows::Here { stage: Stage::Hearing | Stage::Working { line: None }, .. } => None,
            Shows::Here { stage: Stage::Waiting, voice } => match voice {
                VoiceReady::Ready => None,
                VoiceReady::Fetching(None) => Some("Getting the voice ready".into()),
                VoiceReady::Fetching(Some(done)) => Some(format!("Getting the voice ready, {:.0}%", done * 100.)),
                VoiceReady::Failed(why) => Some(format!("Couldn't get the voice ready: {why}")),
            },
            Shows::Here { stage: Stage::Speaking, .. } | Shows::Elsewhere { stage: Some(Stage::Speaking), .. } => {
                Some("Speaking".into())
            }
            Shows::Here { stage: Stage::Working { line: Some(line) }, .. }
            | Shows::Elsewhere { stage: Some(Stage::Working { line: Some(line) }), .. } => Some(line.clone()),
            Shows::Elsewhere { device, .. } => Some(format!("The conversation is on {device}")),
            Shows::MovedTo { device } => Some(format!("The conversation moved to {device}")),
        }
    }

    /// Where the conversation is, said beside what the bar says, while another device has it and the bar says what
    /// it's at there, such as `On Mac Studio`.
    pub fn on(&self) -> Option<String> {
        match self {
            Shows::Elsewhere { device, stage: Some(Stage::Speaking | Stage::Working { line: Some(_) }) } => {
                Some(format!("On {device}"))
            }
            Shows::Composer | Shows::Here { .. } | Shows::Elsewhere { .. } | Shows::MovedTo { .. } => None,
        }
    }
}
