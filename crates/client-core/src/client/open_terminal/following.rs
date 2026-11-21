//! A terminal someone has open, and the source it's followed from.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::TerminalId;
use protocol::terminal::{KeyPress, TerminalSize, TerminalUpdate, Typed};

use super::asking::{self, Waiting};
use super::window::{Ask, Pointer, Replied, SelectBy, Window};
use crate::client::follow::{Follow, Source};
use crate::client::{Client, ClientError, FollowError, Result, Subscription, SubscriptionItem};
use crate::drawn::{Frame, Rgb};

/// A terminal someone has open, followed until every clone of this has gone. One view takes its frames, since taking
/// one empties what's held.
#[derive(Clone)]
pub struct OpenTerminal {
    id: TerminalId,
    pub(super) follow: Follow<Held>,
}

#[derive(Default)]
pub(super) struct Held {
    /// The newest frame, which the view hasn't taken yet.
    frame: Option<Arc<Frame>>,
    /// Whether the view has taken the last frame, so the next can be built. None is built until a view takes one, and
    /// then only once it has taken the one before, so frames never queue.
    wanted: bool,
    /// What views gave the window since its thread last looked, and the replies to what it asked, oldest first.
    pub(super) given: Vec<Given>,
    /// Whether its shell has ended, after which nothing more comes.
    ended: bool,
}

/// What a view gives the window, which only the thread it's followed on reaches, and the replies that thread waits for
/// elsewhere.
pub(super) enum Given {
    Key(KeyPress),
    Paste(String),
    Text(String),
    /// Turns the wheel by this many lines, up for fewer than none.
    Wheel(isize),
    /// The view has room for this size.
    Fit(TerminalSize),
    Colors(Rgb, Rgb),
    /// The pointer's button went down, to select by what the click of a run it was selects by.
    Press(Pointer, SelectBy),
    /// The pointer moved with the button down, since a press on the grid.
    Drag(Pointer),
    Release,
    SelectAll,
    /// What's selected, as text, for the view to copy. Nothing is sent while nothing is.
    SelectedText(flume::Sender<String>),
    Replied(Replied),
}

impl OpenTerminal {
    /// Follows the terminal `id` on a thread of its own, so this doesn't wait for the hub.
    pub fn start(client: &Client, id: &TerminalId) -> Self {
        let (waiting, replies) = flume::unbounded();
        let source = TerminalSource { id: id.clone(), window: Window::new(), waiting, copies: Vec::new() };
        let follow = Follow::start(client, Held::default(), move || source);
        asking::start(id.clone(), replies, follow.giver());
        Self { id: id.clone(), follow }
    }

    pub fn id(&self) -> &TerminalId {
        &self.id
    }

    /// Says that something a view shows may have changed: there's a new frame to take, its shell has ended, or the
    /// connection or an error has. It holds one signal at a time, so what comes while a view is busy gathers into one.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.follow.changed
    }

    /// The newest frame, if one has come since the last was taken, after which the next is built, once anything
    /// changes.
    pub fn take_frame(&self) -> Option<Arc<Frame>> {
        let (frame, was_wanted) =
            self.follow.quietly(|held| (held.frame.take(), std::mem::replace(&mut held.wanted, true)));
        if !was_wanted {
            self.follow.wake();
        }
        frame
    }

    /// Whether its shell has ended.
    pub fn ended(&self) -> bool {
        self.follow.read(|held| held.ended)
    }

    /// A key, which the terminal's worker turns into what the program reads, by the modes it set, sent as
    /// [`Client::terminal_typed`] sends it, once. It also takes the view back to the screen from wherever the history
    /// is scrolled to. Nothing is sent before the first picture.
    pub fn key(&self, press: KeyPress) {
        self.give(Given::Key(press));
    }

    /// Pastes `text`, which the worker marks as pasted for a program that asks, with the control characters that could
    /// end the marks early swapped for spaces, as `protocol::terminal::Typed::Paste` says.
    pub fn paste(&self, text: String) {
        self.give(Given::Paste(text));
    }

    /// Sends `text` as it is, whatever modes the program set, as a key a client gives a meaning of its own sends what it
    /// means, such as Command-Left on a Mac, which goes to the start of the line. Otherwise it's sent as a key is.
    pub fn send_text(&self, text: String) {
        self.give(Given::Text(text));
    }

    /// Turns the wheel by `lines`, up for fewer than none, which scrolls through the history, or is sent to a
    /// full-screen program.
    pub fn wheel(&self, lines: isize) {
        self.give(Given::Wheel(lines));
    }

    /// The view has room for `size`, which the terminal is asked to take if it isn't that size already, and again
    /// whenever it's watched again at another size. A new size another client gives it is kept until the room changes.
    /// The view's top and what's selected stay on their rows through a size it asked for, and go to the bottom with
    /// nothing selected through one another client gave.
    pub fn fit(&self, size: TerminalSize) {
        self.give(Given::Fit(size));
    }

    /// The colours a cell that sets none is drawn in, which a view takes from its own look, from now on, unless the
    /// program set its own. Ghostty's are used until then.
    pub fn set_colors(&self, foreground: Rgb, background: Rgb) {
        self.give(Given::Colors(foreground, background));
    }

    /// The pointer's button went down at `at`, which starts a selection, or selects the word or the line there, as `by`
    /// says, which is what the click of a run it was selects by.
    pub fn press(&self, at: Pointer, by: SelectBy) {
        self.give(Given::Press(at, by));
    }

    /// The pointer moved to `at` with the button down, since a press, which takes the selection there.
    pub fn drag(&self, at: Pointer) {
        self.give(Given::Drag(at));
    }

    pub fn release(&self) {
        self.give(Given::Release);
    }

    /// Selects everything, the history too.
    pub fn select_all(&self) {
        self.give(Given::SelectAll);
    }

    /// What's selected, as text, once the hub has sent it all, as the terminal's worker copies it. Nothing comes while
    /// nothing is selected, or without a connection.
    pub fn selected_text(&self) -> flume::Receiver<String> {
        let (reply, selected) = flume::bounded(1);
        self.give(Given::SelectedText(reply));
        selected
    }

    fn give(&self, given: Given) {
        self.follow.quietly(|held| held.given.push(given));
        self.follow.wake();
    }

    /// Carries on on a new connection, which starts again from a picture of what the terminal shows.
    pub fn reconnect(&self, client: &Client) {
        self.follow.reconnect(client);
    }

    /// True once it has caught up on a connection, and false from when that goes until it has on another.
    pub fn connected(&self) -> bool {
        self.follow.connected()
    }

    pub fn take_errors(&self) -> Vec<FollowError> {
        self.follow.take_errors()
    }
}

struct TerminalSource {
    id: TerminalId,
    window: Window,
    /// The replies to wait for, on the thread that waits for them.
    waiting: flume::Sender<Waiting>,
    /// Where to send what's selected, asked while it was still to be settled, once it is.
    copies: Vec<flume::Sender<String>>,
}

impl Source for TerminalSource {
    type Held = Held;

    fn resume(&mut self, client: &Client, held: &Mutex<Held>) -> Result<Subscription> {
        let watched = client.terminal_watch(&self.id);
        // Its record has gone with its shell, which may have ended while there was no connection to hear it. The
        // window keeps what it showed.
        if watched.as_ref().is_err_and(ClientError::is_not_found) {
            held.lock().ended = true;
        }
        watched
    }

    fn apply(&mut self, client: &Client, held: &Mutex<Held>, item: SubscriptionItem) -> Result<bool> {
        let ended = match item {
            SubscriptionItem::TerminalPicture(picture) => {
                let asks = self
                    .window
                    .take(*picture)
                    .map_err(|_| ClientError::Unexpected("a picture lacked rows the client didn't hold".to_string()))?;
                self.send(Some(client), asks);
                false
            }
            SubscriptionItem::Terminal(TerminalUpdate::Exited(_)) => true,
            // What's sent of a terminal that isn't watched, and what a newer hub sends, change nothing.
            SubscriptionItem::Terminal(_)
            | SubscriptionItem::Event(_)
            | SubscriptionItem::Tail(_)
            | SubscriptionItem::Delta(_)
            | SubscriptionItem::Notice(_)
            | SubscriptionItem::Github(_) => return Ok(false),
        };
        if ended {
            held.lock().ended = true;
        }
        Ok(self.settle(Some(client), held) || ended)
    }

    /// What's given to the window changes it whether or not there's a connection, as the wheel scrolls through the
    /// rows it holds without one. What that would ask of the hub is dropped while there's none, as typing over one
    /// that's gone is.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<Held>, _: &dyn Fn()) -> Result<bool> {
        Ok(self.settle(connection, held))
    }

    fn reconnected(&mut self) {
        self.window.disconnected();
        // Nothing is copied without a connection, as `OpenTerminal::selected_text` says.
        self.copies.clear();
    }
}

impl TerminalSource {
    /// Does what views gave the window, in order, then builds the next frame if the view has taken the last, and
    /// anything has changed since. Returns whether it built one.
    fn settle(&mut self, connection: Option<&Client>, held: &Mutex<Held>) -> bool {
        // Under one lock, so a view that gives something, then takes a frame, as it gives its colours, gets one with
        // what it gave in.
        let (given, wanted) = {
            let mut held = held.lock();
            (std::mem::take(&mut held.given), held.wanted)
        };
        for given in given {
            let asks = self.answer(connection, given);
            self.send(connection, asks);
        }
        if !self.window.selection_coming() {
            for reply in std::mem::take(&mut self.copies) {
                self.copy(connection, reply);
            }
        }
        if !wanted {
            return false;
        }
        let Some(frame) = self.window.frame() else { return false };
        let mut held = held.lock();
        held.frame = Some(frame);
        held.wanted = false;
        true
    }

    fn answer(&mut self, connection: Option<&Client>, given: Given) -> Vec<Ask> {
        let window = &mut self.window;
        match given {
            Given::Key(press) => window.typed(Typed::Key(press)),
            Given::Paste(text) => window.typed(Typed::Paste { text }),
            Given::Text(text) => window.typed(Typed::Text { text }),
            Given::Wheel(lines) => window.wheel(lines),
            Given::Fit(room) => window.fit(room),
            Given::Colors(foreground, background) => {
                window.set_colors(foreground, background);
                Vec::new()
            }
            Given::Press(at, by) => window.press(at, by),
            Given::Drag(at) => window.drag(at),
            Given::Release => {
                window.release();
                Vec::new()
            }
            Given::SelectAll => {
                window.select_all();
                Vec::new()
            }
            Given::SelectedText(reply) => {
                if window.selection_coming() {
                    self.copies.push(reply);
                } else {
                    self.copy(connection, reply);
                }
                Vec::new()
            }
            Given::Replied(replied) => window.replied(replied),
        }
    }

    /// Asks for the text of what's selected, for `reply`, which nothing is sent to while nothing is, or without a
    /// connection.
    fn copy(&self, connection: Option<&Client>, reply: flume::Sender<String>) {
        if let (Some(client), Some((numbering, range))) = (connection, self.window.selected()) {
            let copy = Waiting::Copy { client: client.clone(), numbering, range, reply };
            let _ = self.waiting.send(copy);
        }
    }

    /// Sends what the window asked for. What can't be sent, as while there's no connection, is dropped, and the window
    /// is told, so it asks again for what it still lacks.
    fn send(&mut self, connection: Option<&Client>, asks: Vec<Ask>) {
        for ask in asks {
            let sent = connection.map(|client| asking::send(client, &self.id, ask));
            match sent {
                Some(Ok(Some(waiting))) => {
                    let _ = self.waiting.send(waiting);
                }
                Some(Ok(None)) => {}
                Some(Err(_)) | None => {
                    let asks = self.window.replied(Replied::Failed);
                    debug_assert!(asks.is_empty(), "a failure asks for nothing more");
                }
            }
        }
    }
}
