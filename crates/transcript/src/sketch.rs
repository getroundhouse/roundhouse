//! Which option for how a subagent's call and the subagent tab look this build draws, so the gallery can draw each.
//! A sketch for #417, on a branch that's never merged.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sketch {
    /// The tab is a chat of its own.
    Chat,
    /// The tab is a page about the subagent, as the diff tab is about a file.
    Page,
    /// The tab is the call, opened in full.
    Thread,
}

static CHOSEN: AtomicU8 = AtomicU8::new(0);

pub fn set_sketch(sketch: Sketch) {
    CHOSEN.store(sketch as u8, Ordering::Relaxed);
}

pub fn sketch() -> Sketch {
    match CHOSEN.load(Ordering::Relaxed) {
        1 => Sketch::Page,
        2 => Sketch::Thread,
        _ => Sketch::Chat,
    }
}
