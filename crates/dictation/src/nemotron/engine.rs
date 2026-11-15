//! The engine, loaded: its parts, the features its encoders take, and its vocabulary.

use std::path::Path;

use super::decoding::{Decoder, Encoded};
use super::live::Live;
use super::mel::{MELS, Mel};
use super::parts::{Input, Part, Parts, Tensor};
use super::vocab::Vocabulary;
use super::windows::Steps;
use crate::{Engine, EngineError, Listening, Samples};

/// Where the vocabulary is, in the directory the engine loads from.
pub(super) const VOCABULARY: &str = "coreml/vocab.json";

/// The language the encoders are told they'll hear: English, the first of the model's prompts. *Unconfirmed.*
const ENGLISH: i32 = 0;

/// The longest piece the offline encoder takes: 15 s, as it was exported.
const LONGEST: Samples = Samples::from_secs(15);

/// The oldest macOS that runs the model, since the export is Core ML's specification 8.
pub const OLDEST_MACOS: u8 = 14;

/// Whether this computer can run the model: a Mac with [`OLDEST_MACOS`] or later.
#[cfg(target_os = "macos")]
pub fn runs_here() -> bool {
    use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
    let oldest = NSOperatingSystemVersion { majorVersion: OLDEST_MACOS.into(), minorVersion: 0, patchVersion: 0 };
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(oldest)
}

/// Whether this computer can run the model: only a Mac can.
#[cfg(not(target_os = "macos"))]
pub fn runs_here() -> bool {
    false
}

/// NVIDIA's nemotron-labs-unified-asr-0.6b, on Core ML: live text from its live encoder, 0.4 s or so after each word
/// is said, and each piece's final text from its offline encoder, which hears the piece whole.
pub struct Nemotron {
    pub(super) parts: Parts,
    mel: Mel,
    pub(super) vocabulary: Vocabulary,
}

impl Nemotron {
    /// The engine, from its files in `dir`, as [`super::MODEL`] lists them. The first load after the files are
    /// downloaded has Core ML compile the encoders for the Neural Engine, which takes tens of seconds, and a lot of
    /// memory while it does. Core ML keeps what it compiled, so later loads take a fraction of a second, until it
    /// compiles them again.
    pub fn load(dir: &Path) -> Result<Self, EngineError> {
        let vocabulary = Vocabulary::read(&dir.join(VOCABULARY))?;
        Ok(Self { parts: Parts::load(dir)?, mel: Mel::new(), vocabulary })
    }

    /// The engine, with stand-ins for its parts.
    #[cfg(test)]
    pub(super) fn standing_in(parts: Parts, vocabulary: Vocabulary) -> Self {
        Self { parts, mel: Mel::new(), vocabulary }
    }

    /// What `encoder` makes out of `padded`, a window that starts at `first` in the recording, whose first `audio` is
    /// audio and the rest padding.
    pub(super) fn encode(
        &self,
        encoder: &dyn Part,
        padded: &[f32],
        audio: Samples,
        first: Steps,
    ) -> Result<Encoded, EngineError> {
        let features = self.mel.features(padded, audio);
        let audio_frames = i32::try_from(features.audio_frames).map_err(|_| EngineError("too long".to_string()))?;
        let inputs = [
            Input::Floats { name: "mel", values: &features.values, shape: &[1, MELS, features.frames] },
            Input::Ints { name: "mel_length", values: &[audio_frames], shape: &[1] },
            Input::Ints { name: "prompt_id", values: &[ENGLISH], shape: &[1] },
        ];
        let outputs = encoder.run(&inputs, &["encoder", "encoder_length"])?;
        let [made_out, length] =
            <[Tensor; 2]>::try_from(outputs).map_err(|_| EngineError("an encoder gave too little".to_string()))?;
        Encoded::new(made_out, &length, first)
    }
}

impl Engine for Nemotron {
    fn longest_piece(&self) -> Samples {
        LONGEST
    }

    fn listen(&self) -> Box<dyn Listening + '_> {
        Box::new(Live::new(self))
    }

    fn transcribe(&self, piece: &[f32]) -> Result<String, EngineError> {
        if piece.len() > LONGEST.0 {
            return Err(EngineError(format!("a piece of {} samples is longer than {}", piece.len(), LONGEST.0)));
        }
        let mut padded = vec![0.0; LONGEST.0];
        padded[..piece.len()].copy_from_slice(piece);
        let encoded = self.encode(&*self.parts.offline, &padded, Samples(piece.len()), Steps(0))?;
        let tokens = Decoder::new().decode(&self.parts, &encoded, Steps(0)..encoded.made_out())?;
        Ok(self.vocabulary.join(tokens.into_iter().map(|token| token.id)).trim().to_string())
    }
}
