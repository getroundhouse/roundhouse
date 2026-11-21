//! The voice this device reads with, and its files, which are fetched once speech is turned on, and only then, as
//! `docs/spec/chats.md` says.

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;

use client_core::conversation::{FromVoice, Readiness, ToVoice, Voice, VoiceText};
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
    /// Whether speech is on.
    on: Mutex<bool>,
    /// How the files stand, once speech is on.
    ready: Mutex<Readiness>,
    /// Whether the files are being fetched now.
    fetching: Mutex<bool>,
    /// A test's voice, which has no files and reads nothing.
    silent: bool,
    /// Holds one signal at a time, so changes nobody has looked at gather into one.
    tell: flume::Sender<()>,
    changed: flume::Receiver<()>,
}

impl Voices {
    /// Phonon's voice, with its files kept in `model_dir`, on the device `here` names, such as "this Mac". It's off,
    /// and nothing is fetched, until [`Self::turn_on`].
    pub fn new(model_dir: PathBuf, here: &'static str) -> Self {
        Self::fetched_from(model_dir, here, MODEL.hugging_face())
    }

    /// The same, with the files fetched from `from`, such as a test's server.
    pub fn fetched_from(model_dir: PathBuf, here: &'static str, from: String) -> Self {
        Self::made(model_dir, here, from, false)
    }

    /// A voice that's ready once it's on, with nothing to fetch, and reads nothing, for a test of what uses it.
    #[cfg(any(test, feature = "testing"))]
    pub fn silent() -> Self {
        Self::made(PathBuf::new(), "this device", String::new(), true)
    }

    fn made(model_dir: PathBuf, here: &'static str, from: String, silent: bool) -> Self {
        let (tell, changed) = flume::bounded(1);
        let (on, ready, fetching) = (Mutex::new(false), Mutex::new(Readiness::NotOn), Mutex::new(false));
        Self { shared: Arc::new(Shared { model_dir, from, here, on, ready, fetching, silent, tell, changed }) }
    }

    /// Turns speech on, as the person did, and fetches Phonon's files unless they're here or being fetched, as trying
    /// again after a failure does too.
    pub fn turn_on(&self) {
        *self.shared.on.lock() = true;
        if self.shared.silent || MODEL.is_here(&self.shared.model_dir) {
            self.set(Readiness::Ready);
            return;
        }
        if std::mem::replace(&mut *self.shared.fetching.lock(), true) {
            let _ = self.shared.tell.try_send(());
            return;
        }
        self.set(Readiness::Downloading(None));
        let voices = self.clone();
        let fetch = move || {
            let ready = match voices.fetch() {
                Ok(()) => Readiness::Ready,
                Err(why) => Readiness::Failed(why),
            };
            *voices.shared.fetching.lock() = false;
            voices.set(ready);
        };
        if let Err(e) = std::thread::Builder::new().name("voice-download".into()).spawn(fetch) {
            *self.shared.fetching.lock() = false;
            self.set(Readiness::Failed(format!("it couldn't start: {e}.")));
        }
    }

    /// Turns speech off, as the person did, so nothing more is read. Files being fetched go on coming, since they're
    /// small, and stay for when it's on again.
    pub fn turn_off(&self) {
        if std::mem::replace(&mut *self.shared.on.lock(), false) {
            let _ = self.shared.tell.try_send(());
        }
    }

    /// Fetches the files, saying how far as it goes.
    fn fetch(&self) -> Result<(), String> {
        let mut progress = |now: Progress| {
            #[expect(clippy::cast_precision_loss, reason = "a bar's width needs nothing like 2^24 steps")]
            let done = now.done_bytes as f32 / now.total_bytes.max(1) as f32;
            self.set(Readiness::Downloading(Some(done)));
            ControlFlow::Continue(())
        };
        match MODEL.download(&self.shared.model_dir, &self.shared.from, &mut progress) {
            Ok(Fetched::Done | Fetched::Stopped) => Ok(()),
            Err(e) => Err(e.why(self.shared.here)),
        }
    }

    /// How speech stands: off, getting its files, ready to read, or failed.
    pub fn ready(&self) -> Readiness {
        if !*self.shared.on.lock() {
            return Readiness::NotOn;
        }
        self.shared.ready.lock().clone()
    }

    /// Says that [`Self::ready`] changed. It holds one signal at a time.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.shared.changed
    }

    fn set(&self, ready: Readiness) {
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
        let speaking = (!self.shared.silent).then(|| Speaking::start(move || Phonon::load(&dir)).ok()).flatten();
        Spoken { speaking, voices: self.clone(), never: flume::bounded(0) }
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
                self.voices.set(Readiness::Failed(format!("it couldn't load: {e}.")));
                None
            }
            // A phrase passed over, or speakers that stopped working, which open again for the next.
            Update::Failed(SpeakingError::Phrase(_) | SpeakingError::Speakers(_)) => None,
        }
    }

    /// Only once its files are here, so it never tries to load them meanwhile.
    fn ready(&self) -> bool {
        self.speaking.is_some() && self.voices.ready() == Readiness::Ready
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use client_core::conversation::Readiness;
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
            if let Readiness::Failed(why) = voices.ready() {
                return why;
            }
        };

        assert_eq!(voices.ready(), Readiness::NotOn);
        voices.turn_on();
        assert_eq!(failed(&voices), "Hugging Face answered 404.");
        voices.turn_on();
        assert_eq!(failed(&voices), "Hugging Face answered 404.");
        assert_eq!(server.requests().len(), 2, "asked again");

        voices.turn_off();
        assert_eq!(voices.ready(), Readiness::NotOn, "off says so, whatever the files did");
        assert_eq!(server.requests().len(), 2, "turning it off fetches nothing");
    }
}
