//! Audio awaiting recognition, bounded by samples rather than callback count. Closing wakes capture separately
//! from recognition, so neither a full queue nor a blocked model can delay releasing the microphone.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;

use super::recording::Input;
use crate::DictationError;

/// Thirty seconds leaves room for model startup without letting a stuck recognizer accumulate audio indefinitely.
pub(super) const MAX_SAMPLES: usize = crate::Samples::from_secs(30).0;

#[derive(Clone)]
pub(super) struct Buffer {
    state: Arc<Mutex<State>>,
    wake: flume::Sender<()>,
    ready: flume::Receiver<()>,
    close: flume::Sender<()>,
    closed: flume::Receiver<()>,
}

struct State {
    audio: VecDeque<Vec<f32>>,
    samples: usize,
    accepting: bool,
    ending: Option<Input>,
}

impl Buffer {
    pub(super) fn new() -> Self {
        let (wake, ready) = flume::bounded(1);
        let (close, closed) = flume::bounded(1);
        let state = State { audio: VecDeque::new(), samples: 0, accepting: true, ending: None };
        Self { state: Arc::new(Mutex::new(state)), wake, ready, close, closed }
    }

    pub(super) fn audio(&self, audio: Vec<f32>) {
        let mut state = self.state.lock();
        if !state.accepting || audio.is_empty() {
            return;
        }
        if audio.len() > MAX_SAMPLES - state.samples {
            self.finish(&mut state, Input::Failed(DictationError::AudioBacklog));
            return;
        }
        state.samples += audio.len();
        state.audio.push_back(audio);
        let _ = self.wake.try_send(());
    }

    /// Stop keeps all accepted audio in order. Failure and cancellation discard it, keeping only text already final.
    /// The terminal event has its own slot, so a full audio buffer cannot lose a stop or block its caller.
    pub(super) fn end(&self, ending: Input) {
        self.finish(&mut self.state.lock(), ending);
    }

    fn finish(&self, state: &mut State, ending: Input) {
        if !state.accepting {
            return;
        }
        state.accepting = false;
        if !matches!(ending, Input::Stop) {
            state.audio.clear();
            state.samples = 0;
        }
        state.ending = Some(ending);
        let _ = self.close.try_send(());
        let _ = self.wake.try_send(());
    }

    pub(super) fn next(&self) -> Input {
        loop {
            let mut state = self.state.lock();
            if let Some(audio) = state.audio.pop_front() {
                state.samples -= audio.len();
                return Input::Audio(audio);
            }
            if let Some(ending) = state.ending.take() {
                return ending;
            }
            drop(state);
            let _ = self.ready.recv();
        }
    }

    pub(super) fn accepting(&self) -> bool {
        self.state.lock().accepting
    }

    pub(super) fn closed(&self) -> &flume::Receiver<()> {
        &self.closed
    }
}
