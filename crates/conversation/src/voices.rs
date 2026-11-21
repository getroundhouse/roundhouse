//! The voice this device reads with, and its files.

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;

use client_core::conversation::{FromVoice, ToVoice, Voice, VoiceReady, VoiceText};
use models::{Fetched, ModelFiles, Progress};
use parking_lot::Mutex;
use phonon::{MODEL, Phonon};
use speaking::{Speaking, SpeakingError, TextId, Update};

/// The voice's files, as [`Voices`] keeps them in its folder, such as for a test to put stand-ins for them there.
pub const FILES: ModelFiles = MODEL;

/// Phonon's files, which every chat whose conversation is on this device reads with, and whether they're here.
#[derive(Clone)]
pub struct Voices {
    shared: Arc<Shared>,
}

struct Shared {
    /// Where Phonon's files are kept, by revision, in a folder of their own.
    model_dir: PathBuf,
    /// Where they're fetched from, which serves each at its path.
    from: String,
    /// The device, as words about it name it, such as "this Mac".
    here: &'static str,
    ready: Mutex<VoiceReady>,
    /// Whether the files are being fetched now.
    fetching: Mutex<bool>,
    /// Holds one signal at a time, so changes nobody has looked at gather into one.
    tell: flume::Sender<()>,
    changed: flume::Receiver<()>,
}

impl Voices {
    /// Phonon's voice, with its files kept in `model_dir`, on the device `here` names, such as "this Mac". Nothing is
    /// fetched until [`Self::get_ready`].
    pub fn new(model_dir: PathBuf, here: &'static str) -> Self {
        Self::fetched_from(model_dir, here, MODEL.hugging_face())
    }

    /// The same, with the files fetched from `from`, such as a test's server.
    pub fn fetched_from(model_dir: PathBuf, here: &'static str, from: String) -> Self {
        let (tell, changed) = flume::bounded(1);
        // Until it's asked to get ready, which the conversation coming here does at once.
        let ready = if MODEL.is_here(&model_dir) { VoiceReady::Ready } else { VoiceReady::Fetching(None) };
        let (fetching, ready) = (Mutex::new(false), Mutex::new(ready));
        Self { shared: Arc::new(Shared { model_dir, from, here, ready, fetching, tell, changed }) }
    }

    /// Fetches Phonon's files, unless they're here or being fetched, as the conversation coming to this device does
    /// first, and trying again after a failure does.
    pub fn get_ready(&self) {
        if MODEL.is_here(&self.shared.model_dir) {
            self.set(VoiceReady::Ready);
            return;
        }
        if std::mem::replace(&mut *self.shared.fetching.lock(), true) {
            return;
        }
        self.set(VoiceReady::Fetching(None));
        let voices = self.clone();
        let fetch = move || {
            let ready = match voices.fetch() {
                Ok(()) => VoiceReady::Ready,
                Err(why) => VoiceReady::Failed(why),
            };
            *voices.shared.fetching.lock() = false;
            voices.set(ready);
        };
        if let Err(e) = std::thread::Builder::new().name("voice-download".into()).spawn(fetch) {
            *self.shared.fetching.lock() = false;
            self.set(VoiceReady::Failed(format!("it couldn't start: {e}.")));
        }
    }

    /// Fetches the files, saying how far as it goes.
    fn fetch(&self) -> Result<(), String> {
        let mut progress = |now: Progress| {
            #[expect(clippy::cast_precision_loss, reason = "a bar's width needs nothing like 2^24 steps")]
            let done = now.done_bytes as f32 / now.total_bytes.max(1) as f32;
            self.set(VoiceReady::Fetching(Some(done)));
            ControlFlow::Continue(())
        };
        match MODEL.download(&self.shared.model_dir, &self.shared.from, &mut progress) {
            Ok(Fetched::Done | Fetched::Stopped) => Ok(()),
            Err(e) => Err(e.why(self.shared.here)),
        }
    }

    /// Whether the voice can read.
    pub fn ready(&self) -> VoiceReady {
        self.shared.ready.lock().clone()
    }

    /// Says that [`Self::ready`] changed. It holds one signal at a time.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.shared.changed
    }

    fn set(&self, ready: VoiceReady) {
        let mut now = self.shared.ready.lock();
        if *now != ready {
            *now = ready;
            let _ = self.shared.tell.try_send(());
        }
    }

    /// A voice for one chat's conversation, which loads Phonon the first time it has something to read, once its
    /// files are here.
    pub fn voice(&self) -> Spoken {
        let dir = MODEL.dir(&self.shared.model_dir);
        let speaking = Speaking::start(move || Phonon::load(&dir));
        Spoken { speaking: speaking.ok(), voices: self.clone(), never: flume::bounded(0) }
    }
}

/// Phonon reading out through the speakers, as `client-core`'s reading out has it.
pub struct Spoken {
    /// `None` when its thread couldn't start, so it reads nothing.
    speaking: Option<Speaking>,
    voices: Voices,
    /// What says nothing, while it's kept, in place of what a voice that couldn't start would say.
    never: (flume::Sender<Update>, flume::Receiver<Update>),
}

impl Voice for Spoken {
    type Said = Update;

    fn act(&mut self, what: ToVoice) {
        let Some(speaking) = &self.speaking else { return };
        match what {
            ToVoice::Say { text, at, more } => speaking.say(TextId(text.0), at, &more),
            ToVoice::End(text) => speaking.end(TextId(text.0)),
            ToVoice::Stop => speaking.stop(),
        }
    }

    fn said(&self) -> flume::Receiver<Update> {
        match &self.speaking {
            Some(speaking) => speaking.updates(),
            // One that never ends, so reading out waits on the rest.
            None => self.never.1.clone(),
        }
    }

    fn read(&mut self, said: Update) -> Option<FromVoice> {
        let text = |text: TextId| VoiceText(text.0);
        match said {
            Update::Heard { text: said, through } => Some(FromVoice::Heard { text: text(said), through }),
            Update::Finished { text: said } => Some(FromVoice::Finished(text(said))),
            Update::Stopped { cut, never_started } => Some(FromVoice::Stopped {
                cut: cut.map(|(said, through)| (text(said), through)),
                never_started: never_started.into_iter().map(text).collect(),
            }),
            // The files went, or never came, so they're fetched again.
            Update::Failed(SpeakingError::Load(e)) => {
                self.voices.set(VoiceReady::Failed(format!("it couldn't load: {e}.")));
                None
            }
            // A phrase passed over, or speakers that stopped working, which open again for the next.
            Update::Failed(SpeakingError::Phrase(_) | SpeakingError::Speakers(_)) => None,
        }
    }

    /// Only once its files are here, so it never tries to load them meanwhile.
    fn ready(&self) -> bool {
        self.speaking.is_some() && self.voices.ready() == VoiceReady::Ready
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use client_core::conversation::VoiceReady;
    use fetching::serving::serve;

    use super::Voices;

    /// Files that can't be fetched say why, in words, and asking again fetches them again.
    #[test]
    fn files_that_cant_be_fetched_say_why_and_are_fetched_again_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let server = serve(Vec::new()).unwrap();
        let voices = Voices::fetched_from(dir.path().join("phonon"), "this Mac", server.base.clone());
        let failed = |voices: &Voices| loop {
            voices.changed().recv_timeout(Duration::from_secs(10)).unwrap();
            if let VoiceReady::Failed(why) = voices.ready() {
                return why;
            }
        };

        voices.get_ready();
        assert_eq!(failed(&voices), "Hugging Face answered 404.");
        voices.get_ready();
        assert_eq!(failed(&voices), "Hugging Face answered 404.");
        assert_eq!(server.requests().len(), 2, "asked again");
    }
}
