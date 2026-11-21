//! Reading a chat's replies out on this device, on a thread of its own: it follows the chat, has [`Reading`] say what
//! to read and what to tell the hub, gives the voice what it says, and tells the hub, in order, on another thread, so
//! the voice never waits on the hub.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::event::SessionEvent;
use protocol::model::{Session, SessionId};
use protocol::tail::Tail;

use super::reading::Reading;
use super::telling::{Do, FromVoice, ToHub, ToVoice};
use crate::client::{Client, LiveItem, LiveLog, Reply, Result};

/// The voice an app reads with, such as Phonon through the speakers, as the `conversation` crate gives it.
pub trait Voice: Send + 'static {
    /// What the voice says as it goes, as it says it.
    type Said: Send + 'static;

    /// Has it do `what`.
    fn act(&mut self, what: ToVoice);

    /// What it says as it goes.
    fn said(&self) -> flume::Receiver<Self::Said>;

    /// What reading out takes from what it said, or `None` for what it doesn't, such as a failure, which the voice
    /// says itself where it's shown, and after which it says it stopped, if it did.
    fn read(&mut self, said: Self::Said) -> Option<FromVoice>;

    /// Whether it can read now. What comes while it can't, as while its files are fetched, isn't read out.
    fn ready(&self) -> bool {
        true
    }
}

enum Command {
    /// Stop, or typing.
    Stop,
    /// The person started talking.
    Talk,
    /// The person stopped talking, having sent what they said, or not.
    LetGo { sent: bool },
    /// Turn the mode off: stop the voice, say how each reply ended, then end the mode, and answer with how the hub
    /// took that.
    Leave(flume::Sender<Result<()>>),
}

/// How long leaving waits for the voice to say it stopped, so how far it got lands before the mode ends. One that
/// never answers, as one that never started, leaves the hub to settle it.
const LEAVING_WAITS: Duration = Duration::from_secs(2);

/// What the voice is reading.
const NOTHING: u8 = 0;
const A_LINE: u8 = 1;
const A_REPLY: u8 = 2;

/// Reading the chat's replies out, from when it's made until it [`Self::leave`]s, or is dropped, which stops the voice.
/// Made once this device has the chat's conversation, and dropped once it hasn't. The hub settles what it hadn't said
/// the end of then, as the mode moves or ends, or the connection goes.
pub struct ReadingOut {
    log: LiveLog,
    client: Arc<Mutex<Client>>,
    commands: flume::Sender<Command>,
    reading: Arc<AtomicU8>,
    /// Says there's a new connection, to tell the hub on what a connection that went lost.
    reconnected: flume::Sender<()>,
}

impl ReadingOut {
    /// Starts reading out the replies to come in `session`, with `voice`, telling the hub on `client`, and saying on
    /// `tell` when what it reads changes.
    pub fn start(client: &Client, session: &Session, voice: impl Voice, tell: flume::Sender<()>) -> Self {
        let log = LiveLog::start(client, session);
        let client = Arc::new(Mutex::new(client.clone()));
        let (commands, take) = flume::unbounded();
        let reading = Arc::new(AtomicU8::new(NOTHING));
        let (to_hub, outbox) = flume::unbounded();
        let (reconnected, connection) = flume::bounded(1);
        let (id, telling) = (session.id.clone(), client.clone());
        // A thread that can't start leaves the hub to settle what it'd have said.
        let _ = thread::Builder::new()
            .name("telling-the-hub-what-was-heard".into())
            .spawn(move || tell_the_hub(&id, &telling, &outbox, &connection));
        let follower = Follower {
            log: log.clone(),
            voice,
            commands: take,
            to_hub,
            reading: None,
            before: Vec::new(),
            tail: Tail::default(),
            is_reading: reading.clone(),
            tell,
        };
        // One that can't start reads nothing, as with no voice.
        let _ = thread::Builder::new().name("reading-out".into()).spawn(move || follower.run());
        Self { log, client, commands, reading, reconnected }
    }

    /// Carries on, on a new connection to the hub.
    pub fn reconnect(&self, client: &Client) {
        *self.client.lock() = client.clone();
        self.log.reconnect(client);
        let _ = self.reconnected.try_send(());
    }

    /// Stops what's being read, as Stop and typing do, as [`Reading::stop`] says.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// The person started talking, as [`Reading::talk`] says.
    pub fn talk(&self) {
        let _ = self.commands.send(Command::Talk);
    }

    /// The person stopped talking, as [`Reading::let_go`] says.
    pub fn let_go(&self, sent: bool) {
        let _ = self.commands.send(Command::LetGo { sent });
    }

    /// Turns the mode off: stops the voice, tells the hub how each reply ended, then ends the mode, in that order, on
    /// threads that finish by themselves. Answers with how the hub took ending the mode.
    pub fn leave(self) -> Reply<()> {
        let (answer, reply) = Reply::channel();
        // Reading out that's gone drops the answer, which says the mode didn't end.
        let _ = self.commands.send(Command::Leave(answer));
        reply
    }

    /// Whether the voice has something it hasn't finished, as [`Reading::reading`] says.
    pub fn reading(&self) -> bool {
        self.reading.load(Ordering::Acquire) != NOTHING
    }

    /// Whether what the voice hasn't finished includes a reply, rather than only a line said for the agent.
    pub fn reading_a_reply(&self) -> bool {
        self.reading.load(Ordering::Acquire) == A_REPLY
    }
}

/// What the thread that reads holds.
struct Follower<V> {
    log: LiveLog,
    voice: V,
    commands: flume::Receiver<Command>,
    to_hub: flume::Sender<Tell>,
    /// `None` until the chat's tail has come, so what was recorded before is known.
    reading: Option<Reading>,
    /// What was recorded before the tail came, until it has.
    before: Vec<SessionEvent>,
    /// The chat's tail, as the hub sent it and kept up to date here as the log does.
    tail: Tail,
    is_reading: Arc<AtomicU8>,
    tell: flume::Sender<()>,
}

/// What to tell the hub, in turn.
enum Tell {
    Hub(ToHub),
    /// Turn the mode off, and answer with how the hub took it.
    End(flume::Sender<Result<()>>),
}

enum Woken<S> {
    Changed,
    Said(S),
    Command(Command),
    /// What it waits on has gone: the [`ReadingOut`], or the voice.
    Gone,
    /// Leaving has waited as long as it does.
    Late,
}

impl<V: Voice> Follower<V> {
    fn run(mut self) {
        let said = self.voice.said();
        // Once leaving, until the voice has said it stopped, or it's waited long enough.
        let mut leaving: Option<Instant> = None;
        // Once leaving, what to answer with how the hub took the end.
        let mut ending = None;
        loop {
            let selector = flume::Selector::new()
                .recv(self.log.changed(), |changed| changed.map_or(Woken::Gone, |()| Woken::Changed))
                .recv(&said, |said| said.map_or(Woken::Gone, Woken::Said));
            let woken = match leaving {
                // The commands are done with once leaving, and their sender may have gone.
                Some(until) => selector.wait_deadline(until).unwrap_or(Woken::Late),
                None => selector.recv(&self.commands, |command| command.map_or(Woken::Gone, Woken::Command)).wait(),
            };
            let done = match woken {
                Woken::Changed => self.log.take().into_iter().flat_map(|item| self.came(item)).collect(),
                Woken::Said(said) => match (self.voice.read(said), self.reading.as_mut()) {
                    (Some(from), Some(reading)) => reading.voice(from),
                    _ => Vec::new(),
                },
                Woken::Command(command) => {
                    if let Command::Leave(answer) = &command {
                        ending = Some(answer.clone());
                    }
                    let Some(reading) = self.reading.as_mut() else {
                        // Before the tail has come, nothing is being read, and leaving only ends the mode.
                        if ending.is_some() {
                            break;
                        }
                        continue;
                    };
                    match command {
                        Command::Stop => reading.stop(),
                        Command::Talk => reading.talk(),
                        Command::LetGo { sent } => {
                            reading.let_go(sent);
                            Vec::new()
                        }
                        Command::Leave(_) => {
                            leaving = Some(Instant::now() + LEAVING_WAITS);
                            reading.stop()
                        }
                    }
                }
                Woken::Gone | Woken::Late => break,
            };
            self.carry_out(done);
            if leaving.is_some() && !self.reading.as_ref().is_some_and(Reading::reading) {
                break;
            }
        }
        self.voice.act(ToVoice::Stop);
        if let Some(answer) = ending {
            let _ = self.to_hub.send(Tell::End(answer));
        }
        self.set_reading(NOTHING);
    }

    /// What reading out says of `item`, the next the hub sent.
    fn came(&mut self, item: LiveItem) -> Vec<Do> {
        match item {
            LiveItem::Before(event) => {
                self.before.push(*event);
                return Vec::new();
            }
            LiveItem::Tail(tail) => self.tail = *tail,
            LiveItem::Delta(delta) => self.tail.apply(&delta),
            LiveItem::Event(event) => {
                self.tail.on_event(event.seq, event.run, &event.body);
                if let Some(reading) = self.reading.as_mut() {
                    let mut done = reading.recorded(&event);
                    done.extend(reading.streaming(&self.tail));
                    return done;
                }
            }
        }
        if let Some(reading) = self.reading.as_mut() {
            return reading.streaming(&self.tail);
        }
        // The first tail: what was recorded before it, and what's under way in it, isn't read.
        let mut reading = Reading::from(&self.tail);
        for event in self.before.drain(..) {
            reading.before(&event);
        }
        self.reading = Some(reading);
        Vec::new()
    }

    /// Does what reading out said, in order. What's for a voice that can't read yet is told back to the reading as
    /// unread, which may say more to do.
    fn carry_out(&mut self, mut done: Vec<Do>) {
        while !done.is_empty() {
            let mut unread = Vec::new();
            for what in done {
                match what {
                    Do::Voice(ToVoice::Say { text, .. } | ToVoice::End(text)) if !self.voice.ready() => {
                        if !unread.contains(&text) {
                            unread.push(text);
                        }
                    }
                    Do::Voice(what) => self.voice.act(what),
                    Do::Hub(what) => {
                        let _ = self.to_hub.send(Tell::Hub(what));
                    }
                }
            }
            done = match self.reading.as_mut() {
                Some(reading) => unread.into_iter().flat_map(|text| reading.voice(FromVoice::Unread(text))).collect(),
                None => Vec::new(),
            };
        }
        let now = match self.reading.as_ref() {
            Some(reading) if reading.reading_a_reply() => A_REPLY,
            Some(reading) if reading.reading() => A_LINE,
            Some(_) | None => NOTHING,
        };
        self.set_reading(now);
    }

    fn set_reading(&self, now: u8) {
        if self.is_reading.swap(now, Ordering::AcqRel) != now {
            let _ = self.tell.try_send(());
        }
    }
}

/// Tells the hub each of `outbox` in turn, so how a reply ended lands after how far it got, and before the mode ends.
/// What a connection that went lost is told again on the next, once `connection` says there's one, except how far a
/// reply has got, which the next says anyway. What the hub refuses isn't told again: it settles what the device
/// doesn't say.
fn tell_the_hub(
    id: &SessionId,
    client: &Mutex<Client>,
    outbox: &flume::Receiver<Tell>,
    connection: &flume::Receiver<()>,
) {
    while let Ok(what) = outbox.recv() {
        let told = loop {
            let client = client.lock().clone();
            let told = match &what {
                Tell::Hub(ToHub::Speaking(through)) => client.session_speaking(id, through.clone()).map(drop),
                Tell::Hub(ToHub::Heard { message, ending }) => {
                    client.session_reply_heard(id, message.clone(), *ending).map(drop)
                }
                Tell::End(_) => client.session_conversation_end(id).map(drop),
            };
            let lost = told.is_err() && client.is_disconnected() && !matches!(what, Tell::Hub(ToHub::Speaking(_)));
            // Once the reading out has gone, no new connection will be said.
            if !lost || connection.recv().is_err() {
                break told;
            }
        };
        if let Tell::End(answer) = what {
            // Whoever asked may no longer listen.
            let _ = answer.send(told);
        }
    }
}
