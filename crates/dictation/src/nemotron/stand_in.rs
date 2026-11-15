//! Stand-ins for the model's parts, to test what runs them without Core ML: encoders that make out a step of nothing
//! for each 80 ms of audio, a decoder whose output is how many tokens it has heard, and a joint that goes by that.

use super::decoding::{BLANK, ENCODED, SAID};
use super::parts::{Input, Part, Parts, Tensor};
use crate::EngineError;

/// `n`, as a part gives a count.
pub(super) fn count(n: usize) -> Tensor {
    Tensor { values: vec![real(n)], shape: vec![1] }
}

#[expect(clippy::cast_precision_loss, reason = "small counts in tests")]
pub(super) fn real(n: usize) -> f32 {
    n as f32
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "small counts in tests")]
fn whole(value: f32) -> usize {
    value as usize
}

/// Parts whose joint gives tokens, numbered from 1, until as many have been heard as the step says there should be by
/// its end.
pub(super) fn counting_parts() -> Parts {
    parts(|step, heard| if heard < step { heard + 1 } else { BLANK })
}

/// Parts whose joint never says a step is done, giving token 1 every time.
pub(super) fn never_done_parts() -> Parts {
    parts(|_, _| 1)
}

/// Parts whose joint gives a token from the value of a step and how many tokens have been heard.
fn parts(joint: fn(usize, usize) -> usize) -> Parts {
    Parts {
        live: Box::new(Encoder),
        offline: Box::new(Encoder),
        decoder: Box::new(Counter),
        joint: Box::new(Joint(joint)),
    }
}

/// Refuses an input whose values don't fill its shape, as Core ML does.
fn shaped(inputs: &[Input<'_>]) -> Result<(), EngineError> {
    for input in inputs {
        let (name, count, shape) = match input {
            Input::Floats { name, values, shape } => (name, values.len(), shape),
            Input::Ints { name, values, shape } => (name, values.len(), shape),
        };
        if count != shape.iter().product::<usize>() {
            return Err(EngineError(format!("{name} has {count} values for a shape of {shape:?}")));
        }
    }
    Ok(())
}

/// The input named `wanted`.
fn input<'a>(inputs: &'a [Input<'a>], wanted: &str) -> Option<&'a Input<'a>> {
    inputs.iter().find(|input| {
        let (Input::Floats { name, .. } | Input::Ints { name, .. }) = input;
        *name == wanted
    })
}

fn floats<'a>(inputs: &'a [Input<'a>], wanted: &str) -> &'a [f32] {
    match input(inputs, wanted) {
        Some(Input::Floats { values, .. }) => values,
        Some(Input::Ints { .. }) | None => &[],
    }
}

/// The first value of the input named `wanted`, which holds a count or a token's number.
fn int(inputs: &[Input<'_>], wanted: &str) -> usize {
    match input(inputs, wanted) {
        Some(Input::Ints { values, .. }) => values.first().map_or(0, |&n| usize::try_from(n).unwrap_or(0)),
        Some(Input::Floats { .. }) | None => 0,
    }
}

/// An encoder that makes out a step of zeros for each 8 of the features' frames, as the model's do.
struct Encoder;

impl Part for Encoder {
    fn run(&self, inputs: &[Input<'_>], _: &[&str]) -> Result<Vec<Tensor>, EngineError> {
        shaped(inputs)?;
        let Some(Input::Floats { shape: [1, _, frames], .. }) = input(inputs, "mel") else {
            return Err(EngineError("no features".to_string()));
        };
        let steps = frames.div_ceil(8);
        let encoder = Tensor { values: vec![0.0; ENCODED * steps], shape: vec![1, ENCODED, steps] };
        Ok(vec![encoder, count(int(inputs, "mel_length").div_ceil(8))])
    }
}

/// A decoder whose output starts with how many tokens it has heard, which it carries in its state.
struct Counter;

impl Part for Counter {
    fn run(&self, inputs: &[Input<'_>], _: &[&str]) -> Result<Vec<Tensor>, EngineError> {
        shaped(inputs)?;
        let token = int(inputs, "targets");
        let hidden = floats(inputs, "h_in");
        let heard = hidden.first().copied().unwrap_or_default() + if token == BLANK { 0.0 } else { 1.0 };
        let mut out = vec![0.0; SAID];
        out[0] = heard;
        let mut state = hidden.to_vec();
        state[0] = heard;
        let (shape, cell) = (vec![2, 1, 640], floats(inputs, "c_in").to_vec());
        Ok(vec![
            Tensor { values: out, shape: vec![1, SAID, 1] },
            Tensor { values: state, shape: shape.clone() },
            Tensor { values: cell, shape },
        ])
    }
}

/// A joint that gives a token from the value of the step and how many tokens the decoder has heard.
struct Joint(fn(usize, usize) -> usize);

impl Part for Joint {
    fn run(&self, inputs: &[Input<'_>], _: &[&str]) -> Result<Vec<Tensor>, EngineError> {
        shaped(inputs)?;
        let step = whole(floats(inputs, "encoder_step").first().copied().unwrap_or_default());
        let heard = whole(floats(inputs, "decoder_step").first().copied().unwrap_or_default());
        Ok(vec![count((self.0)(step, heard))])
    }
}
