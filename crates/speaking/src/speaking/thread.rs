//! The handle to the thread, and what the thread does with each command and with what the output says.

use std::io;
use std::ops::ControlFlow;
use std::time::Duration;

use phonon::{Phonon, PhononError};
use sound::{Played, Speakers, SpeakersError};

use super::parts::{Output, Voice};
use crate::reader::{Phrase, Reader};
use crate::{SpeakingError, TextId, Update};

/// How long the speakers stay open once everything given has gone to them: longer than they take to play what they
/// hold, so the end of a reply isn't cut off, and long enough that a reply's next block doesn't open them again.
const CLOSE_AFTER: Duration = Duration::from_secs(1);

/// Reading out, on a thread of its own, until this is dropped.
pub struct Speaking {
    commands: flume::Sender<Command>,
    updates: flume::Receiver<Update>,
}

enum Command {
    Say(TextId, usize, String),
    End(TextId),
    Stop,
}

impl Speaking {
    /// Starts the thread, which loads the voice with `load` when it's first given something to say, and again after it
    /// failed.
    pub fn start(load: impl Fn() -> Result<Phonon, PhononError> + Send + 'static) -> io::Result<Self> {
        Self::start_with(load, Speakers::open, CLOSE_AFTER)
    }

    /// Starts the thread with a voice and an output of the caller's, which it opens at the voice's rate, and closes
    /// after `close_after` with nothing playing.
    pub(crate) fn start_with<V: Voice + 'static, O: Output + 'static>(
        load: impl Fn() -> Result<V, PhononError> + Send + 'static,
        open: impl Fn(u32) -> Result<O, SpeakersError> + Send + 'static,
        close_after: Duration,
    ) -> io::Result<Self> {
        let (commands, taken) = flume::unbounded();
        let (said, updates) = flume::unbounded();
        std::thread::Builder::new().name("speaking".to_string()).spawn(move || {
            let mut thread = Thread {
                load: Box::new(load),
                open: Box::new(open),
                close_after,
                reader: Reader::default(),
                voice: None,
                said,
            };
            thread.run(&taken);
        })?;
        Ok(Self { commands, updates })
    }

    /// Reads `more` of `text`, which starts `at` bytes into it, after every text given before it. A text starts with
    /// what's given at 0, and what doesn't follow on from what's been given is dropped, such as the rest of a text
    /// stopped while it was still coming.
    pub fn say(&self, text: TextId, at: usize, more: &str) {
        let _ = self.commands.send(Command::Say(text, at, more.to_string()));
    }

    /// No more of `text` is coming, so it's read to its end.
    pub fn end(&self, text: TextId) {
        let _ = self.commands.send(Command::End(text));
    }

    /// Stops at once, and forgets every text, which [`Update::Stopped`] then says.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// What it says as it reads, which any thread or async task can wait on. Each update goes to one receiver, so a
    /// clone of it takes updates from the other.
    pub fn updates(&self) -> flume::Receiver<Update> {
        self.updates.clone()
    }
}

/// The thread's own state, but the output, which [`Thread::run`] keeps while it's open.
struct Thread<V, O> {
    load: Box<dyn Fn() -> Result<V, PhononError> + Send>,
    open: Box<dyn Fn(u32) -> Result<O, SpeakersError> + Send>,
    close_after: Duration,
    reader: Reader,
    voice: Option<V>,
    said: flume::Sender<Update>,
}

enum Event {
    Command(Command),
    Played(Played),
    /// Nothing has played for [`Thread::close_after`].
    Quiet,
    /// The handle has gone, so nobody is listening.
    Gone,
}

/// How making a phrase ended.
enum Made {
    Played,
    Stopped,
    Failed(SpeakingError),
}

impl<V: Voice, O: Output> Thread<V, O> {
    fn run(&mut self, commands: &flume::Receiver<Command>) {
        let mut output = None;
        loop {
            while let Some(phrase) = self.reader.next() {
                self.make(&phrase, &mut output, commands);
            }
            match self.wait(commands, output.as_ref()) {
                Event::Command(command) => {
                    if self.take(command).is_break() {
                        self.stop(&mut output);
                    }
                }
                Event::Played(Played::Reached(mark)) => {
                    let heard = self.reader.reached(mark);
                    self.tell(heard);
                }
                // With nothing being read, an output that failed is only closed, and the next text opens it again.
                Event::Played(Played::Failed(_)) if !self.reader.is_reading() => output = None,
                Event::Played(Played::Failed(failure)) => self.fail(failure.into(), &mut output),
                Event::Quiet => output = None,
                Event::Gone => return,
            }
        }
    }

    /// The next command, what the open output says, or, once it has nothing to play, that it's been quiet a while.
    fn wait(&self, commands: &flume::Receiver<Command>, output: Option<&O>) -> Event {
        let selector = flume::Selector::new().recv(commands, |command| command.map_or(Event::Gone, Event::Command));
        let Some(output) = output else { return selector.wait() };
        let selector = selector.recv(output.played(), |played| {
            Event::Played(played.unwrap_or_else(|_| Played::Failed(SpeakersError::Failed("it closed".to_string()))))
        });
        if self.reader.is_playing() {
            selector.wait()
        } else {
            selector.wait_timeout(self.close_after).unwrap_or(Event::Quiet)
        }
    }

    /// Takes a command, but a stop, which it leaves to its caller.
    fn take(&mut self, command: Command) -> ControlFlow<()> {
        match command {
            Command::Say(text, at, more) => self.reader.say(text, at, &more),
            Command::End(text) => self.reader.end(text),
            Command::Stop => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    /// Makes `phrase` and plays it, then its mark, opening the output first if it's closed.
    fn make(&mut self, phrase: &Phrase, output: &mut Option<O>, commands: &flume::Receiver<Command>) {
        let mut open = match output.take() {
            Some(open) => open,
            // Nothing to play, and nothing playing before it, since the output is closed: it's heard at once.
            None if phrase.say.is_empty() => {
                let heard = self.reader.reached(phrase.mark);
                self.tell(heard);
                return;
            }
            None => match self.open() {
                Ok(open) => open,
                Err(failure) => return self.fail(failure, output),
            },
        };
        let made = self.play(phrase, &mut open, commands);
        *output = Some(open);
        match made {
            Made::Played => {}
            Made::Stopped => self.stop(output),
            Made::Failed(failure) => self.fail(failure, output),
        }
    }

    /// The voice, loaded, and the output, opened at its rate.
    fn open(&mut self) -> Result<O, SpeakingError> {
        let rate = self.voice()?.sample_rate();
        Ok((self.open)(rate)?)
    }

    fn voice(&mut self) -> Result<&V, SpeakingError> {
        let voice = match self.voice.take() {
            Some(voice) => voice,
            None => (self.load)().map_err(SpeakingError::Load)?,
        };
        Ok(self.voice.insert(voice))
    }

    /// Plays `phrase` through `output`, then its mark. Between each piece of its audio, it passes on what the output
    /// has reached and takes any command, so a stop cuts it short.
    fn play(&mut self, phrase: &Phrase, output: &mut O, commands: &flume::Receiver<Command>) -> Made {
        if !phrase.say.is_empty() {
            let speech = match self.voice() {
                Ok(voice) => voice.speak(&phrase.say),
                Err(failure) => return Made::Failed(failure),
            };
            match speech {
                Ok(speech) => {
                    for audio in speech {
                        if let Err(failure) = self.take_played(output) {
                            return Made::Failed(failure.into());
                        }
                        while let Ok(command) = commands.try_recv() {
                            if self.take(command).is_break() {
                                return Made::Stopped;
                            }
                        }
                        match audio {
                            Ok(audio) => output.play(&audio),
                            Err(e) => {
                                self.tell([Update::Failed(SpeakingError::Phrase(e))]);
                                break;
                            }
                        }
                    }
                }
                // A phrase the voice can't say is passed over, and what follows it is read.
                Err(e) => self.tell([Update::Failed(SpeakingError::Phrase(e))]),
            }
        }
        output.mark(phrase.mark);
        Made::Played
    }

    /// Passes on each mark `output` has reached since it was last asked, or says it failed.
    fn take_played(&mut self, output: &O) -> Result<(), SpeakersError> {
        for played in output.played().try_iter() {
            match played {
                Played::Reached(mark) => {
                    let heard = self.reader.reached(mark);
                    self.tell(heard);
                }
                Played::Failed(failure) => return Err(failure),
            }
        }
        Ok(())
    }

    /// Stops at once, and says how far each text was heard, counting what the output reached before it stopped.
    fn stop(&mut self, output: &mut Option<O>) {
        if let Some(open) = output {
            open.stop();
            if self.take_played(open).is_err() {
                *output = None;
            }
        }
        let stopped = self.reader.stop();
        self.tell([stopped]);
    }

    /// The voice or the output failed: says so, closes the output and stops, so what's read next opens it again.
    fn fail(&mut self, failure: SpeakingError, output: &mut Option<O>) {
        *output = None;
        let stopped = self.reader.stop();
        self.tell([Update::Failed(failure), stopped]);
    }

    fn tell(&self, updates: impl IntoIterator<Item = Update>) {
        for update in updates {
            let _ = self.said.send(update);
        }
    }
}
