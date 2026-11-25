//! The microphone has its own lifetime, independent of model calls. Only this thread opens and drops it; the
//! recording handle, overflow and device failure all wake it directly, without going through recognition.

use std::any::Any;
use std::sync::Arc;

use sound::{InputDevice, Microphone, MicrophoneError};

use super::buffer::Buffer;
use super::recording::Input;
use crate::{DictationError, SAMPLE_RATE};

/// Opens a capture resource on its owning thread. Tests supply a watched resource through the same path, without
/// opening the person's microphone.
pub(super) type Open = Arc<dyn Fn(Buffer) -> Result<Opened, DictationError> + Send + Sync>;

/// What capture opened. Its concrete type matters only to its destructor.
pub(super) struct Opened {
    pub(super) resource: Box<dyn Any>,
    pub(super) device: InputDevice,
}

pub(super) struct Capture(Buffer);

impl Capture {
    /// Starts capture, and gives the device it hears, unless the recording had ended before it opened one.
    pub(super) fn start(buffer: Buffer, open: Open) -> Result<(Self, Option<InputDevice>), DictationError> {
        let (started, ready) = flume::bounded(1);
        let capture = buffer.clone();
        std::thread::Builder::new()
            .name("dictation-microphone".to_string())
            .spawn(move || {
                if !capture.accepting() {
                    let _ = started.send(Ok(None));
                    return;
                }
                match open(capture.clone()) {
                    Ok(Opened { resource, device }) => {
                        let _ = started.send(Ok(Some(device)));
                        let _ = capture.closed().recv();
                        drop(resource);
                    }
                    Err(error) => {
                        let _ = started.send(Err(error));
                    }
                }
            })
            .map_err(|error| MicrophoneError::Failed(error.to_string()))?;
        let device =
            ready.recv().map_err(|_| MicrophoneError::Failed("capture stopped while opening".to_string()))??;
        Ok((Self(buffer), device))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.0.end(Input::Cancel);
    }
}

pub(super) fn microphone(buffer: Buffer) -> Result<Opened, DictationError> {
    let failed = buffer.clone();
    let microphone = Microphone::open(
        SAMPLE_RATE,
        move |audio| buffer.audio(audio),
        move |error| failed.end(Input::Failed(error.into())),
    )?;
    let device = microphone.device().clone();
    Ok(Opened { resource: Box::new(microphone), device })
}
