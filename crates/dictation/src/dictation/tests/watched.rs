//! The fake engine, watched by a test, and held where the test says.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::fake::FakeEngine;
use crate::{Engine, EngineError, Samples};

/// Somewhere a test holds the dictation thread: it says once it's there, and waits to be let go.
#[derive(Clone)]
pub(super) struct Gate {
    reached: flume::Sender<()>,
    open: flume::Receiver<()>,
}

impl Gate {
    /// The gate, what says it was reached, and what opens it.
    pub(super) fn new() -> (Self, flume::Receiver<()>, flume::Sender<()>) {
        let ((reached, at_gate), (opens, open)) = (flume::unbounded(), flume::unbounded());
        (Self { reached, open }, at_gate, opens)
    }

    pub(super) fn pass(&self) {
        let _ = self.reached.send(());
        let _ = self.open.recv();
    }
}

/// What a test sees of a [`Watched`] engine.
#[derive(Clone)]
pub(super) struct Watch {
    /// How many recordings it has started hearing.
    listened: Arc<AtomicUsize>,
    /// How many samples of audio it has heard live.
    heard: Arc<AtomicUsize>,
    /// Told each time an engine is let go of.
    let_go: flume::Sender<()>,
}

impl Watch {
    /// The watch, and what's told each time an engine is let go of.
    pub(super) fn new() -> (Self, flume::Receiver<()>) {
        let (let_go, gone) = flume::unbounded();
        (Self { listened: Arc::default(), heard: Arc::default(), let_go }, gone)
    }

    pub(super) fn listened(&self) -> usize {
        self.listened.load(Ordering::SeqCst)
    }

    pub(super) fn heard(&self) -> usize {
        self.heard.load(Ordering::SeqCst)
    }
}

/// The fake engine, watched, and held at `gate`, if it has one, each time it starts on a piece's final text.
pub(super) struct Watched {
    pub(super) engine: FakeEngine,
    pub(super) watch: Watch,
    pub(super) gate: Option<Gate>,
}

struct WatchedListening<'e> {
    listening: Box<dyn crate::Listening + 'e>,
    heard: &'e AtomicUsize,
}

impl Engine for Watched {
    fn longest_piece(&self) -> Samples {
        self.engine.longest_piece()
    }

    fn listen(&self, hints: &crate::Hints, language: crate::Language) -> Box<dyn crate::Listening + '_> {
        self.watch.listened.fetch_add(1, Ordering::SeqCst);
        Box::new(WatchedListening { listening: self.engine.listen(hints, language), heard: &self.watch.heard })
    }

    fn transcribe(
        &self,
        piece: &[f32],
        hints: &crate::Hints,
        language: crate::Language,
    ) -> Result<String, EngineError> {
        if let Some(gate) = &self.gate {
            gate.pass();
        }
        self.engine.transcribe(piece, hints, language)
    }
}

impl crate::Listening for WatchedListening<'_> {
    fn hear(&mut self, audio: &[f32]) -> Result<Vec<crate::Heard>, EngineError> {
        self.heard.fetch_add(audio.len(), Ordering::SeqCst);
        self.listening.hear(audio)
    }

    fn finish(&mut self) -> Result<Vec<crate::Heard>, EngineError> {
        self.listening.finish()
    }

    fn heard_through(&self) -> Samples {
        self.listening.heard_through()
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        let _ = self.watch.let_go.send(());
    }
}
