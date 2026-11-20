use std::sync::atomic::{AtomicU8, Ordering};

/// How far away the screen is held, which every foundation has a value for: a desk's, or a handheld's, held closer.
///
/// It's the device's, so it's chosen once for the whole process, as the app starts and before anything is drawn, with
/// [`Reach::choose`], and is a desk's until then. It's kept for the process rather than in the app because a size
/// becomes a length where there's no app to ask, as in `.h(look::RowSize::Regular.height())`. So a process draws
/// for one reach: the desktop app and its tests a desk's, and the iOS app and its tests a handheld's. *Unconfirmed.*
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reach {
    Desk,
    Handheld,
}

/// The reach chosen, as [`Reach::code`] gives it.
static CHOSEN: AtomicU8 = AtomicU8::new(Reach::Desk.code());

impl Reach {
    /// The reach this process draws for.
    pub fn now() -> Reach {
        if CHOSEN.load(Ordering::Relaxed) == Reach::Handheld.code() { Reach::Handheld } else { Reach::Desk }
    }

    /// Draws everything this process draws for this reach, from now on. The app calls it once, before
    /// [`crate::apply`], which gives the kit's theme the reach's rem.
    pub fn choose(self) {
        CHOSEN.store(self.code(), Ordering::Relaxed);
    }

    const fn code(self) -> u8 {
        match self {
            Reach::Desk => 0,
            Reach::Handheld => 1,
        }
    }
}
