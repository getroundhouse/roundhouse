//! The model's parts: the live and offline encoders, run on the Neural Engine through Core ML, and [`Transducer`],
//! which runs the prediction and joint networks in Rust from their weight files.

use std::path::Path;

use super::transducer::{RustTransducer, Transducer};
use crate::EngineError;

/// One input to an encoder part.
#[cfg_attr(
    all(not(target_os = "macos"), not(test)),
    expect(dead_code, reason = "only Core ML reads a part's inputs, and only on a Mac")
)]
pub(super) enum Input<'a> {
    Floats { name: &'static str, values: &'a [f32], shape: &'a [usize] },
    Ints { name: &'static str, values: &'a [i32], shape: &'a [usize] },
}

/// One output of an encoder part: its values, first dimension first, and its shape.
pub(super) struct Tensor {
    pub(super) values: Vec<f32>,
    pub(super) shape: Vec<usize>,
}

/// An encoder part of the model, loaded and ready to run: on Core ML, or, in tests, a stand-in from `stand_in`.
pub(super) trait Part {
    /// Runs it on `inputs`, and gives the outputs named in `outputs`, in their order.
    fn run(&self, inputs: &[Input<'_>], outputs: &[&str]) -> Result<Vec<Tensor>, EngineError>;
}

pub(super) struct Parts {
    pub(super) live: Box<dyn Part>,
    pub(super) offline: Box<dyn Part>,
    pub(super) transducer: Box<dyn Transducer>,
}

/// Where each part is, in the directory the engine loads from, as `super::files` lists them.
pub(super) const LIVE: &str = "coreml/nemotron_unified_encoder_streaming_70_2_5.mlmodelc";
pub(super) const OFFLINE: &str = "coreml/nemotron_unified_encoder.mlmodelc";
pub(super) const DECODER: &str = "coreml/nemotron_unified_decoder.mlmodelc";
pub(super) const JOINT: &str = "coreml/nemotron_unified_joint_decision_single_step.mlmodelc";

impl Parts {
    /// The parts from the files in `dir`.
    pub(super) fn load(dir: &Path) -> Result<Self, EngineError> {
        Ok(Self {
            live: load_encoder(&dir.join(LIVE))?,
            offline: load_encoder(&dir.join(OFFLINE))?,
            transducer: Box::new(RustTransducer::load(dir)?),
        })
    }
}

#[cfg(target_os = "macos")]
fn load_encoder(path: &Path) -> Result<Box<dyn Part>, EngineError> {
    Ok(Box::new(super::coreml::CoreMlPart::load(path)?))
}

#[cfg(not(target_os = "macos"))]
fn load_encoder(_: &Path) -> Result<Box<dyn Part>, EngineError> {
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
