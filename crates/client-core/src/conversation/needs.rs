//! What conversation mode needs on a device, as `docs/spec/chats.md` says: dictation, to hear the person, and speech,
//! to read the agent's replies out. Each is set up on its own, and the mode works once both are ready. How each stands
//! is said here, in the same words on every client, whatever hears and speaks on it.

/// Dictation or speech.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Dictation,
    Speech,
}

impl Part {
    /// Both, in the order they're listed.
    pub const ALL: [Self; 2] = [Self::Dictation, Self::Speech];

    /// What it's called, as a list of them starts it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Dictation => "Dictation",
            Self::Speech => "Speech",
        }
    }

    /// What it's called in a sentence.
    fn called(self) -> &'static str {
        match self {
            Self::Dictation => "dictation",
            Self::Speech => "speech",
        }
    }
}

/// How dictation or speech stands on this device.
#[derive(Clone, Debug, PartialEq)]
pub enum Readiness {
    /// It isn't on, so nothing has been downloaded for it.
    NotOn,
    /// Its files are being downloaded, this far, from 0 to 1, once that's known.
    Downloading(Option<f32>),
    /// Its model is loading.
    Preparing,
    Ready,
    /// It couldn't be made ready, for this reason, and can be tried again.
    Failed(String),
}

impl Readiness {
    /// How it stands, as a row of the card that sets up conversation mode says it.
    pub fn says(&self) -> String {
        match self {
            Self::NotOn => "Not on".into(),
            Self::Downloading(None) => "Downloading".into(),
            Self::Downloading(Some(done)) => format!("Downloading, {}", percent(*done)),
            Self::Preparing => "Preparing".into(),
            Self::Ready => "Ready".into(),
            Self::Failed(why) => format!("Couldn't get it ready: {why}"),
        }
    }

    /// Whether it's on its way to ready by itself.
    fn getting_ready(&self) -> bool {
        matches!(self, Self::Downloading(_) | Self::Preparing)
    }
}

/// How dictation and speech stand on this device, which conversation mode needs both of.
#[derive(Clone, Debug, PartialEq)]
pub struct Needs {
    pub dictation: Readiness,
    pub speech: Readiness,
}

impl Needs {
    /// Both ready.
    pub const READY: Self = Self { dictation: Readiness::Ready, speech: Readiness::Ready };

    /// How `part` stands.
    pub fn of(&self, part: Part) -> &Readiness {
        match part {
            Part::Dictation => &self.dictation,
            Part::Speech => &self.speech,
        }
    }

    /// Whether the mode can work: both are ready.
    pub fn ready(&self) -> bool {
        Part::ALL.into_iter().all(|part| *self.of(part) == Readiness::Ready)
    }

    /// The parts that aren't on, which turning on downloads.
    pub fn not_on(&self) -> Vec<Part> {
        self.parts(|readiness| *readiness == Readiness::NotOn)
    }

    /// What the button that turns on what isn't says, or `None` when both are on.
    pub fn turn_on(&self) -> Option<String> {
        match self.not_on().as_slice() {
            [] => None,
            [part] => Some(format!("Turn on {}", part.called())),
            [..] => Some("Turn on both".into()),
        }
    }

    /// The first part that couldn't be made ready, which trying again tries.
    pub fn failed(&self) -> Option<Part> {
        Part::ALL.into_iter().find(|part| matches!(self.of(*part), Readiness::Failed(_)))
    }

    /// What the bar says while the conversation is here and they aren't both ready, as when it came here from another
    /// device first: what failed, then what isn't on, then what's getting ready. `None` once both are.
    pub fn says(&self) -> Option<String> {
        if let Some(part) = self.failed()
            && let Readiness::Failed(why) = self.of(part)
        {
            return Some(format!("Couldn't get {} ready: {why}", part.called()));
        }
        match self.not_on().as_slice() {
            [] => {}
            [Part::Speech] => return Some("Turn on speech to hear replies".into()),
            [Part::Dictation] => return Some("Turn on dictation to talk".into()),
            [..] => return Some("Turn on dictation and speech to talk".into()),
        }
        match self.parts(Readiness::getting_ready).as_slice() {
            [] => None,
            [part] => Some(match self.of(*part) {
                Readiness::Downloading(Some(done)) => format!("Getting {} ready, {}", part.called(), percent(*done)),
                Readiness::Downloading(None)
                | Readiness::Preparing
                | Readiness::NotOn
                | Readiness::Ready
                | Readiness::Failed(_) => format!("Getting {} ready", part.called()),
            }),
            [..] => Some("Getting dictation and speech ready".into()),
        }
    }

    fn parts(&self, which: impl Fn(&Readiness) -> bool) -> Vec<Part> {
        Part::ALL.into_iter().filter(|part| which(self.of(*part))).collect()
    }
}

/// `done`, from 0 to 1, as a whole percentage.
fn percent(done: f32) -> String {
    format!("{:.0}%", done * 100.)
}
