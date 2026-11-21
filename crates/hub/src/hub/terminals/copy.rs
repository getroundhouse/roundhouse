//! The hub's copy of a watched terminal, from its worker's pictures: the screen of the newest whole picture, which each
//! client watching is sent what it lacks of, and the rows that have left the screen since the hub was sent them, which
//! say whether a client's copy of a row that left its screen is right. Nothing older, since the history is the
//! worker's, and a client asks it for history by number.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::terminal::{
    Numbering, RowJson, RowNumber, ScreenState, Shown, TerminalCursor, TerminalExit, TerminalPicture, TerminalSize,
};
use tokio::sync::watch;

/// The most rows that have left the screen the hub keeps, and the most bytes of them, which is a few screens of the
/// largest, so a client that's a few pictures behind isn't told to let go of its history. One further behind lets go,
/// and asks the worker for what it scrolls back to.
const MOST_KEPT_ROWS: usize = 1_000;
const MOST_KEPT_BYTES: usize = 8 * 1024 * 1024;

/// What a client watching the terminal is sent from: the newest whole picture, and the shell's end, once it has.
#[derive(Clone, Debug, Default)]
pub(crate) struct Watched {
    pub(crate) shown: Option<Arc<Shown>>,
    pub(crate) exited: Option<TerminalExit>,
}

/// The rows that have left the screen, as the hub had them, from where the worker says they're history as it is on.
#[derive(Debug, Default)]
pub(crate) struct Kept {
    numbering: Option<Numbering>,
    rows: BTreeMap<RowNumber, RowJson>,
    bytes: usize,
}

impl Kept {
    /// The row with `number` in `numbering`, as the terminal keeps it, if the hub has it.
    pub(crate) fn row(&self, numbering: Numbering, number: RowNumber) -> Option<RowJson> {
        (self.numbering == Some(numbering)).then(|| self.rows.get(&number).cloned()).flatten()
    }

    fn clear(&mut self, numbering: Option<Numbering>) {
        *self = Self { numbering, ..Self::default() };
    }

    fn keep(&mut self, number: RowNumber, row: RowJson) {
        self.bytes += row.len();
        if let Some(was) = self.rows.insert(number, row) {
            self.bytes -= was.len();
        }
        while self.rows.len() > MOST_KEPT_ROWS || self.bytes > MOST_KEPT_BYTES {
            let Some((_, oldest)) = self.rows.pop_first() else { break };
            self.bytes -= oldest.len();
        }
    }

    /// Lets go of every row before `number`, which the worker no longer says is history as it is.
    fn let_go_before(&mut self, number: RowNumber) {
        let kept = self.rows.split_off(&number);
        let gone = std::mem::replace(&mut self.rows, kept);
        self.bytes -= gone.values().map(RowJson::len).sum::<usize>();
    }
}

/// A picture whose frames have begun to come, and not its last.
#[derive(Debug)]
struct Gathering {
    numbering: Numbering,
    size: TerminalSize,
    top: RowNumber,
    fresh: bool,
    rows: HashMap<RowNumber, RowJson>,
}

impl Gathering {
    fn of(picture: &TerminalPicture) -> Self {
        let (numbering, size, top, fresh) = (picture.numbering, picture.size, picture.top, picture.fresh);
        Self { numbering, size, top, fresh, rows: HashMap::new() }
    }

    fn is_of(&self, picture: &TerminalPicture) -> bool {
        (self.numbering, self.size, self.top, self.fresh)
            == (picture.numbering, picture.size, picture.top, picture.fresh)
    }
}

/// Why a whole picture couldn't be taken in.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Incomplete;

/// Whether the copy is of the worker's pictures as they are now, which begin again with a fresh one each time the
/// worker is asked to watch. What comes before that fresh picture is for a watch that ended.
#[derive(Debug, PartialEq, Eq)]
enum Fresh {
    /// The worker is to be asked for a fresh picture, as before the first watch, and after a picture the hub
    /// couldn't take in.
    ToAsk,
    /// The worker has been asked.
    Asked,
    Taken,
}

pub(crate) struct Copy {
    fresh: Fresh,
    gathering: Option<Gathering>,
    newest: watch::Sender<Watched>,
    kept: Arc<Mutex<Kept>>,
}

impl Default for Copy {
    fn default() -> Self {
        let (newest, kept) = (watch::Sender::new(Watched::default()), Arc::default());
        Self { fresh: Fresh::ToAsk, gathering: None, newest, kept }
    }
}

impl Copy {
    /// The newest whole picture, from now on, for a client that watches.
    pub(crate) fn watched(&self) -> watch::Receiver<Watched> {
        self.newest.subscribe()
    }

    pub(crate) fn kept(&self) -> Arc<Mutex<Kept>> {
        self.kept.clone()
    }

    /// Whether the worker is to be asked for a fresh picture, which it sends as it's asked to watch.
    pub(crate) fn wants_fresh(&self) -> bool {
        self.fresh == Fresh::ToAsk
    }

    /// As the worker is asked to watch, after which it begins with a fresh picture.
    pub(crate) fn asked(&mut self) {
        self.fresh = Fresh::Asked;
        self.gathering = None;
    }

    fn ask_again(&mut self) {
        self.fresh = Fresh::ToAsk;
        self.gathering = None;
    }

    /// Lets go of what it has, once nobody watches.
    pub(crate) fn clear(&mut self) {
        self.ask_again();
        self.kept.lock().clear(None);
        self.newest.send_modify(|watched| watched.shown = None);
    }

    /// The shell's end, for every client watching.
    pub(crate) fn exited(&self, exit: TerminalExit) {
        self.newest.send_modify(|watched| watched.exited = Some(exit));
    }

    /// Takes in a frame of a picture, and on its last, makes the picture the newest. A picture that lacks a row the hub
    /// doesn't have is `Incomplete`, after which the worker is to be asked for a fresh one.
    pub(crate) fn take(&mut self, picture: TerminalPicture) -> Result<(), Incomplete> {
        if picture.fresh {
            self.fresh = Fresh::Taken;
        } else if self.fresh != Fresh::Taken {
            return Ok(());
        }
        let mut gathering = match self.gathering.take() {
            Some(gathering) if gathering.is_of(&picture) => gathering,
            Some(_) | None => Gathering::of(&picture),
        };
        let TerminalPicture { rows, whole, kept_from, first, cursor, state, .. } = picture;
        for run in rows {
            gathering.rows.extend((run.from.get()..).map(RowNumber::new).zip(run.rows));
        }
        if !whole {
            self.gathering = Some(gathering);
            return Ok(());
        }
        let newest = self.newest.borrow().shown.clone();
        let base = newest
            .filter(|base| !gathering.fresh && (base.numbering, base.size) == (gathering.numbering, gathering.size));
        let Some(shown) = screen(gathering, base.as_deref(), Ending { first, cursor, state }) else {
            self.ask_again();
            return Err(Incomplete);
        };
        self.keep_what_left(base.as_deref(), &shown, kept_from);
        self.newest.send_modify(|watched| watched.shown = Some(Arc::new(shown)));
        Ok(())
    }

    /// Keeps the rows of `before` that have left the screen as of `now`, from `kept_from` on, which the worker says are
    /// history as it is, and lets go of what it kept before `kept_from`.
    fn keep_what_left(&self, before: Option<&Shown>, now: &Shown, kept_from: RowNumber) {
        let mut kept = self.kept.lock();
        if kept.numbering != Some(now.numbering) || before.is_none() {
            kept.clear(Some(now.numbering));
        }
        kept.let_go_before(kept_from.max(now.first));
        let Some(before) = before else { return };
        for (number, row) in (before.top.get()..).map(RowNumber::new).zip(&before.rows) {
            if number >= now.top {
                break;
            }
            if number >= kept_from {
                kept.keep(number, row.clone());
            }
        }
    }
}

/// What a picture's last frame says of the screen besides its rows.
struct Ending {
    first: RowNumber,
    cursor: Option<TerminalCursor>,
    state: Option<ScreenState>,
}

/// The screen a whole picture makes, from its rows and those of `base` it didn't send, or `None` if it lacks one.
fn screen(mut gathering: Gathering, base: Option<&Shown>, ending: Ending) -> Option<Shown> {
    let rows = (gathering.top.get()..gathering.top.get() + u64::from(gathering.size.rows))
        .map(RowNumber::new)
        .map(|number| gathering.rows.remove(&number).or_else(|| base.and_then(|base| held(base, number))))
        .collect::<Option<Vec<_>>>()?;
    let state = ending.state.or_else(|| base.map(|base| base.state.clone()))?;
    Some(Shown {
        numbering: gathering.numbering,
        size: gathering.size,
        top: gathering.top,
        first: ending.first,
        rows,
        cursor: ending.cursor,
        state,
    })
}

fn held(shown: &Shown, number: RowNumber) -> Option<RowJson> {
    let at = usize::try_from(number.get().checked_sub(shown.top.get())?).ok()?;
    shown.rows.get(at).cloned()
}
