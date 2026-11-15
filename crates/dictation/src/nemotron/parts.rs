//! The model's four parts, as the engine calls them, whatever runs them: the live and the offline encoders, which make
//! out what's in the audio, and the decoder and the joint, which turn that into tokens.

use std::path::Path;

use crate::EngineError;

/// One input to a part.
#[cfg_attr(
    all(not(target_os = "macos"), not(test)),
    expect(dead_code, reason = "only Core ML reads a part's inputs, and only on a Mac")
)]
pub(super) enum Input<'a> {
    Floats { name: &'static str, values: &'a [f32], shape: &'a [usize] },
    Ints { name: &'static str, values: &'a [i32], shape: &'a [usize] },
}

/// One output of a part: its values, first dimension first, and its shape.
pub(super) struct Tensor {
    pub(super) values: Vec<f32>,
    pub(super) shape: Vec<usize>,
}

/// A part of the model, loaded and ready to run: on Core ML, or, in tests, a stand-in from `stand_in`.
pub(super) trait Part {
    /// Runs it on `inputs`, and gives the outputs named in `outputs`, in their order.
    fn run(&self, inputs: &[Input<'_>], outputs: &[&str]) -> Result<Vec<Tensor>, EngineError>;
}

/// Where a part runs: the encoders on the Neural Engine, which measured fastest, with the least CPU, and the decoder
/// and the joint on the CPU, since they're small and called many times a window, where the Neural Engine's cost to
/// start each call would outweigh it.
#[derive(Clone, Copy)]
pub(super) enum Runs {
    OnTheNeuralEngine,
    OnTheCpu,
}

pub(super) struct Parts {
    pub(super) live: Box<dyn Part>,
    pub(super) offline: Box<dyn Part>,
    pub(super) decoder: Box<dyn Part>,
    pub(super) joint: Box<dyn Part>,
}

/// Where each part is, in the directory the engine loads from, as `super::files` lists them.
const LIVE: &str = "coreml/nemotron_unified_encoder_streaming_70_2_5.mlmodelc";
const OFFLINE: &str = "coreml/nemotron_unified_encoder.mlmodelc";
const DECODER: &str = "coreml/nemotron_unified_decoder.mlmodelc";
const JOINT: &str = "coreml/nemotron_unified_joint_decision_single_step.mlmodelc";
#[cfg(test)]
pub(super) const PATHS: [&str; 4] = [LIVE, OFFLINE, DECODER, JOINT];

impl Parts {
    /// The parts from the files in `dir`.
    pub(super) fn load(dir: &Path) -> Result<Self, EngineError> {
        Ok(Self {
            live: load(&dir.join(LIVE), Runs::OnTheNeuralEngine)?,
            offline: load(&dir.join(OFFLINE), Runs::OnTheNeuralEngine)?,
            decoder: load(&dir.join(DECODER), Runs::OnTheCpu)?,
            joint: load(&dir.join(JOINT), Runs::OnTheCpu)?,
        })
    }
}

#[cfg(target_os = "macos")]
fn load(path: &Path, runs: Runs) -> Result<Box<dyn Part>, EngineError> {
    Ok(Box::new(super::coreml::CoreMlPart::load(path, runs)?))
}

#[cfg(not(target_os = "macos"))]
fn load(_: &Path, _: Runs) -> Result<Box<dyn Part>, EngineError> {
    Err(EngineError("this model runs only on a Mac".to_string()))
}

impl Tensor {
    /// Its first value, which holds a count or a token's number.
    pub(super) fn whole(&self) -> Result<usize, EngineError> {
        let value = self.values.first().copied().ok_or_else(|| EngineError("a part gave nothing".to_string()))?;
        if value.is_finite() && value >= 0.0 {
            Ok(truncated(value))
        } else {
            Err(EngineError(format!("a part gave {value} for a count")))
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a count or a token's number, which a part gives as a whole number, checked above zero and finite"
)]
fn truncated(value: f32) -> usize {
    value as usize
}
