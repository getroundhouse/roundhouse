//! A Mac's microphone, through Core Audio as `cpal` reaches it, and its permission, through `AVFoundation`.

use block2::RcBlock;
use cpal::traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _};
use cpal::{ErrorKind, SampleFormat};
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use parking_lot::Mutex;

use super::{InputDevice, MicrophoneError};
use crate::Permission;
use crate::resample::Resampler;

/// The system's default input device, heard until this is dropped.
pub struct Microphone {
    _stream: cpal::Stream,
    device: InputDevice,
}

impl Microphone {
    /// Starts hearing the default input device. Core Audio calls `heard` on a thread of its own with each buffer, at
    /// `rate` in one channel, and `failed` if the device stops working, such as when it's unplugged.
    pub fn open(
        rate: u32,
        mut heard: impl FnMut(Vec<f32>) + Send + 'static,
        mut failed: impl FnMut(MicrophoneError) + Send + 'static,
    ) -> Result<Self, MicrophoneError> {
        // Without permission, Core Audio records silence rather than failing, which would read as a quiet room.
        match microphone_permission() {
            Permission::Allowed => {}
            Permission::Denied => return Err(MicrophoneError::Denied),
            Permission::NotAsked => return Err(MicrophoneError::NotAsked),
        }
        let device = cpal::default_host().default_input_device().ok_or(MicrophoneError::Missing)?;
        let config = device.default_input_config().map_err(|e| failure(&e))?;
        let mut resampler = Resampler::new(config.sample_rate(), config.channels(), rate);
        let name = device.description().ok().map(|description| description.name().to_string());
        let input = InputDevice { name, sample_rate: config.sample_rate(), channels: config.channels() };
        let on_error = move |e: cpal::Error| {
            // A few samples lost, and the stream carries on. Anything else ends it, a change of device included, since
            // the resampler is for this device's rate and channels.
            if e.kind() != ErrorKind::Xrun {
                failed(failure(&e));
            }
        };
        let format = config.sample_format();
        let stream = if format == SampleFormat::F32 {
            device.build_input_stream::<f32, _, _>(
                config.config(),
                move |data, _| heard(resampler.push(data)),
                on_error,
                None,
            )
        } else if format == SampleFormat::I16 {
            device.build_input_stream::<i16, _, _>(
                config.config(),
                move |data, _| {
                    let data: Vec<f32> = data.iter().map(|&s| f32::from(s) / 32_768.0).collect();
                    heard(resampler.push(&data));
                },
                on_error,
                None,
            )
        } else {
            return Err(MicrophoneError::Failed(format!("it gives its samples as {format}")));
        }
        .map_err(|e| failure(&e))?;
        stream.play().map_err(|e| failure(&e))?;
        Ok(Self { _stream: stream, device: input })
    }

    /// The device it hears, for as long as it does, since a change of device ends it.
    pub fn device(&self) -> &InputDevice {
        &self.device
    }
}

/// What `cpal`'s error means here.
fn failure(e: &cpal::Error) -> MicrophoneError {
    // Compared rather than matched, since `cpal` can add kinds, and every other is the device failing.
    if e.kind() == ErrorKind::PermissionDenied {
        MicrophoneError::Denied
    } else if e.kind() == ErrorKind::DeviceNotAvailable {
        MicrophoneError::Missing
    } else {
        MicrophoneError::Failed(e.to_string())
    }
}

/// What `AVFoundation` says of the app using the microphone. A build run from a checkout is the terminal's to allow,
/// since a Mac asks of the app that started it.
pub fn microphone_permission() -> Permission {
    // SAFETY: AVFoundation's constant, which it sets once as it loads and never changes.
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return Permission::Denied };
    // SAFETY: a class method that only reads the system's record, from any thread.
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
        AVAuthorizationStatus::Authorized => Permission::Allowed,
        AVAuthorizationStatus::NotDetermined => Permission::NotAsked,
        _ => Permission::Denied,
    }
}

/// Has the system ask the person whether the app may use the microphone, if it hasn't yet, and calls `answered` with
/// whether it may, on a thread of the system's. Asked again, the system answers at once with what was said before.
pub fn ask_for_microphone(answered: impl FnOnce(bool) + Send + 'static) {
    // SAFETY: as in `microphone_permission`.
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return answered(false) };
    // The system calls the block once, but its type lets it call it again, so `answered` is taken out on the first.
    let answered = Mutex::new(Some(answered));
    let block = RcBlock::new(move |allowed: Bool| {
        if let Some(answered) = answered.lock().take() {
            answered(allowed.as_bool());
        }
    });
    // SAFETY: the block is kept by AVFoundation until it has called it, and takes only what's `Send`.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &block) };
}
