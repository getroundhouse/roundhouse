//! A terminal someone has open: the client's copy of what it shows, kept from what the hub sends of it on the thread
//! it's followed on, which starts again from what the terminal shows once the subscription falls behind, or there's a
//! new connection. A view takes a plain copy of what it shows, and gives the copy what it's given, such as keys, which
//! the copy turns into what the program reads, by the modes it set, and sends on.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::TerminalId;
use protocol::terminal::TerminalUpdate;
use terminal_state::drawn::{Frame, Rgb};
use terminal_state::keys::KeyPress;
use terminal_state::{Pointer, SelectBy, Size};

use self::copy::{TerminalCopy, ToHub};
use super::follow::{Follow, Source};
use super::{Client, ClientError, FollowError, Result, Subscription, SubscriptionItem};

mod copy;
#[cfg(test)]
mod tests;

/// A terminal someone has open, followed until every clone of this has gone. One view takes its frames, since taking
/// one empties what's held.
#[derive(Clone)]
pub struct OpenTerminal {
    id: TerminalId,
    follow: Follow<Held>,
}

#[derive(Default)]
struct Held {
    /// The newest frame, which the view hasn't taken yet.
    frame: Option<Arc<Frame>>,
    /// Whether the view has taken the last frame, so the next can be built. None is built until a view takes one, and
    /// then only once it has taken the one before, so frames never queue.
    wanted: bool,
    /// What views gave the copy since its thread last looked, oldest first.
    given: Vec<Given>,
    /// Whether its shell has ended, after which nothing more comes.
    ended: bool,
}

/// What a view gives the copy, which only the thread it's followed on can reach, since Ghostty's terminal can't leave
/// the thread that makes it.
enum Given {
    Key(KeyPress),
    Paste(String),
    /// Turns the wheel by this many lines, up for fewer than none.
    Wheel(isize),
    /// The view has room for this size.
    Fit(Size),
    Colors(Rgb, Rgb),
    /// The pointer's button went down, to select by what the click of a run it was selects by.
    Press(Pointer, SelectBy),
    /// The pointer moved with the button down, since a press on the grid.
    Drag(Pointer),
    Release(Pointer),
    SelectAll,
    /// What's selected, as text, for the view to copy. Nothing is sent while nothing is.
    SelectedText(flume::Sender<String>),
}

impl OpenTerminal {
    /// Follows the terminal `id` on a thread of its own, so this doesn't wait for the hub.
    pub fn start(client: &Client, id: &TerminalId) -> Self {
        let followed = id.clone();
        // Made on the thread it's followed on, since Ghostty's terminal can't leave the thread that makes it.
        let source = move || TerminalSource { id: followed, copy: TerminalCopy::new() };
        Self { id: id.clone(), follow: Follow::start(client, Held::default(), source) }
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

    /// A key, which the copy turns into what the program reads, by the modes it set, and sends, as
    /// [`Client::terminal_input`] does, once. One the program reads anything for also goes back to the screen from
    /// wherever the history is scrolled to. Nothing is sent before the first screen, whose modes it's read by.
    pub fn key(&self, press: KeyPress) {
        self.give(Given::Key(press));
    }

    /// Pastes `text`, marked as pasted for a program that asks, with the control characters that could end the marks
    /// early swapped for spaces, as [`terminal_state::Mirror::paste`] says.
    pub fn paste(&self, text: String) {
        self.give(Given::Paste(text));
    }

    /// Turns the wheel by `lines`, up for fewer than none, which scrolls through the history, or is sent to a
    /// full-screen program.
    pub fn wheel(&self, lines: isize) {
        self.give(Given::Wheel(lines));
    }

    /// The view has room for `size`, which the terminal is asked to take if it isn't that size already, and again
    /// whenever it's attached at another size. A new size another client gives it is kept until the room changes.
    pub fn fit(&self, size: Size) {
        self.give(Given::Fit(size));
    }

    /// The colours a cell that sets none is drawn in, which a view takes from its own look, from now on. Ghostty's are
    /// used until then.
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

    pub fn release(&self, at: Pointer) {
        self.give(Given::Release(at));
    }

    /// Selects everything, the history too.
    pub fn select_all(&self) {
        self.give(Given::SelectAll);
    }

    /// What's selected, as text, once the thread it's followed on has read it, since only that thread can read the
    /// copy. Nothing comes while nothing is selected.
    pub fn selected_text(&self) -> flume::Receiver<String> {
        let (reply, selected) = flume::bounded(1);
        self.give(Given::SelectedText(reply));
        selected
    }

    fn give(&self, given: Given) {
        self.follow.quietly(|held| held.given.push(given));
        self.follow.wake();
    }

    /// Carries on on a new connection, which starts again from what the terminal shows.
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
    copy: TerminalCopy,
}

impl Source for TerminalSource {
    type Held = Held;

    fn resume(&mut self, client: &Client, held: &Mutex<Held>) -> Result<Subscription> {
        let attached = client.terminal_attach(&self.id);
        // Its record has gone with its shell, which may have ended while there was no connection to hear it. The copy
        // keeps what it showed.
        if attached.as_ref().is_err_and(ClientError::is_not_found) {
            held.lock().ended = true;
        }
        attached
    }

    fn apply(&mut self, client: &Client, held: &Mutex<Held>, item: SubscriptionItem) -> Result<bool> {
        let SubscriptionItem::Terminal(update) = item else { return Ok(false) };
        let ended = matches!(update, TerminalUpdate::Exited(_));
        // Ghostty fails only when it can't allocate, or for a size of zero, which a worker refuses, so a failure
        // leaves what it shows as it was, until the next screen.
        if let Ok(to_hub) = self.copy.take_in(update) {
            self.send(Some(client), &to_hub);
        }
        if ended {
            held.lock().ended = true;
        }
        Ok(self.settle(Some(client), held) || ended)
    }

    /// What's given to the copy changes it whether or not there's a connection, as the wheel scrolls the history
    /// without one. What that would send the hub is dropped while there's none, as typing over one that's gone is.
    fn asked(&mut self, connection: Option<&Client>, held: &Mutex<Held>) -> Result<bool> {
        Ok(self.settle(connection, held))
    }
}

impl TerminalSource {
    /// Does what views gave the copy, in order, then builds the next frame if the view has taken the last, and
    /// anything has changed since. Returns whether it built one.
    fn settle(&mut self, connection: Option<&Client>, held: &Mutex<Held>) -> bool {
        // Under one lock, so a view that gives something, then takes a frame, as it gives its colours, gets one with
        // what it gave in.
        let (given, wanted) = {
            let mut held = held.lock();
            (std::mem::take(&mut held.given), held.wanted)
        };
        for given in given {
            let to_hub = self.answer(given);
            self.send(connection, &to_hub);
        }
        if !wanted {
            return false;
        }
        // A frame fails only when Ghostty can't allocate, and the copy tries again with what comes next.
        let Ok(Some(frame)) = self.copy.frame() else { return false };
        let mut held = held.lock();
        held.frame = Some(frame);
        held.wanted = false;
        true
    }

    /// Any of these fails only when Ghostty can't allocate, which leaves the copy as it was, so what failed is dropped,
    /// as a key lost on the way would be, a copy copies nothing, and the next goes on.
    fn answer(&mut self, given: Given) -> ToHub {
        let to_hub = match given {
            Given::Key(press) => self.copy.key(&press),
            Given::Paste(text) => self.copy.paste(&text),
            Given::Wheel(lines) => self.copy.wheel(lines),
            Given::Fit(fit) => self.copy.fit(fit),
            Given::Colors(foreground, background) => {
                self.copy.set_colors(foreground, background).map(|()| ToHub::default())
            }
            Given::Press(at, by) => self.copy.press(at, by).map(|()| ToHub::default()),
            Given::Drag(at) => self.copy.drag(at).map(|()| ToHub::default()),
            Given::Release(at) => self.copy.release(at).map(|()| ToHub::default()),
            Given::SelectAll => self.copy.select_all().map(|()| ToHub::default()),
            Given::SelectedText(reply) => {
                if let Ok(Some(text)) = self.copy.selected_text() {
                    let _ = reply.send(text);
                }
                return ToHub::default();
            }
        };
        to_hub.unwrap_or_default()
    }

    /// Sends what was typed, and a new size. Either is dropped while there's no connection, as typing over one that's
    /// gone is, and the next screen asks for the size again.
    fn send(&self, connection: Option<&Client>, to_hub: &ToHub) {
        let Some(client) = connection else { return };
        if !to_hub.input.is_empty() {
            let _ = client.terminal_input(&self.id, &to_hub.input);
        }
        if let Some(size) = to_hub.resize {
            let _ = client.terminal_resize(&self.id, size);
        }
    }
}
