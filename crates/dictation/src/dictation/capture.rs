//! The microphone has its own lifetime, independent of model calls. Only this thread opens and drops it; the
//! recording handle, overflow and device failure all wake it directly, without going through recognition.

use std::any::Any;
use std::sync::Arc;

use super::buffer::Buffer;
use super::recording::Input;
use crate::DictationError;
use crate::audio::Microphone;

/// Opens a capture resource on its owning thread. Its concrete type matters only to its destructor. Tests supply
/// a watched resource through the same path, without opening the person's microphone.
pub(super) type Open = Arc<dyn Fn(Buffer) -> Result<Box<dyn Any>, DictationError> + Send + Sync>;

pub(super) struct Capture(Buffer);

impl Capture {
    pub(super) fn start(buffer: Buffer, open: Open) -> Result<Self, DictationError> {
        let (started, ready) = flume::bounded(1);
        let capture = buffer.clone();
        std::thread::Builder::new()
            .name("dictation-microphone".to_string())
            .spawn(move || {
                if !capture.accepting() {
                    let _ = started.send(Ok(()));
                    return;
                }
                match open(capture.clone()) {
                    Ok(microphone) => {
                        let _ = started.send(Ok(()));
                        let _ = capture.closed().recv();
                        drop(microphone);
                    }
                    Err(error) => {
                        let _ = started.send(Err(error));
                    }
                }
            })
            .map_err(|error| DictationError::Microphone(error.to_string()))?;
        ready.recv().map_err(|_| DictationError::Microphone("capture stopped while opening".to_string()))??;
        Ok(Self(buffer))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.0.end(Input::Cancel);
    }
}

pub(super) fn microphone(buffer: Buffer) -> Result<Box<dyn Any>, DictationError> {
    let failed = buffer.clone();
    Microphone::open(move |audio| buffer.audio(audio), move |error| failed.end(Input::Failed(error)))
        .map(|microphone| Box::new(microphone) as Box<dyn Any>)
}
