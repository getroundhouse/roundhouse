//! Stand-ins for the model's parts, to test what runs them without Core ML: encoders that make out a step of nothing
//! for each 80 ms of audio, and scripted transducers for deterministic and ambiguous beam-search paths.

use std::sync::Arc;

use parking_lot::Mutex;

use super::engine::LONGEST;
use super::mel::HOP;
use super::parts::{Input, Part, Parts, Tensor};
use super::transducer::{BLANK, ENCODED, HIDDEN, PredState, Transducer, VOCAB_SIZE, log_softmax};
use super::windows::WINDOW;
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

/// Parts whose transducer gives tokens, numbered from 1, until as many have been heard as the step says there should
/// be by its end.
pub(super) fn counting_parts() -> Parts {
    deterministic_parts(|step, heard| if heard < step { heard + 1 } else { BLANK })
}

/// Parts whose transducer never says a step is done, giving token 1 every time.
pub(super) fn never_done_parts() -> Parts {
    deterministic_parts(|_, _| 1)
}

pub(super) fn parts_with_transducer(transducer: impl Transducer + 'static) -> Parts {
    parts_noting_prompts(transducer).0
}

/// Each prompt the encoders were given, in order, with which encoder was: `"live"` or `"offline"`.
pub(super) type Prompts = Arc<Mutex<Vec<(&'static str, usize)>>>;

/// Parts with `transducer`, and the prompts their encoders are given.
pub(super) fn parts_noting_prompts(transducer: impl Transducer + 'static) -> (Parts, Prompts) {
    let prompts = Prompts::default();
    let encoder = |which, expected_frames| Box::new(Encoder { which, expected_frames, prompts: prompts.clone() });
    let parts = Parts {
        live: encoder("live", WINDOW.0 / HOP + 1),
        offline: encoder("offline", LONGEST.0 / HOP + 1),
        transducer: Box::new(transducer),
    };
    (parts, prompts)
}

fn deterministic_parts(choose: fn(usize, usize) -> usize) -> Parts {
    parts_with_transducer(Deterministic(choose))
}

/// A scripted transducer whose step `s`, given the tokens emitted so far, assigns relative logits to `(token, logit)`
/// choices, with every unlisted token at `-30.0` and `BLANK` at `0.0` unless overridden.
pub(super) struct ScriptedTransducer<F>(pub(super) F);

impl<F> Transducer for ScriptedTransducer<F>
where
    F: Fn(usize, &[usize]) -> Vec<(usize, f32)>,
{
    fn start(&self) -> Result<PredState, EngineError> {
        Ok(PredState { joint_pred: vec![0.0; HIDDEN], hidden: Vec::new(), cell: Vec::new() })
    }

    fn advance(&self, state: &PredState, token: usize) -> Result<PredState, EngineError> {
        let mut hidden = state.hidden.clone();
        hidden.push(real(token));
        Ok(PredState { joint_pred: vec![0.0; HIDDEN], hidden, cell: Vec::new() })
    }

    fn project_encoder(&self, step: &[f32]) -> Result<Vec<f32>, EngineError> {
        let mut proj = vec![0.0; HIDDEN];
        proj[0] = step.first().copied().unwrap_or_default();
        Ok(proj)
    }

    fn log_probs(&self, enc_proj: &[f32], states: &[&PredState], out: &mut [f32]) -> Result<(), EngineError> {
        let step = whole(enc_proj[0]);
        for (state, row) in states.iter().zip(out.as_chunks_mut::<VOCAB_SIZE>().0) {
            row.fill(-30.0);
            row[BLANK] = 0.0;
            let history: Vec<usize> = state.hidden.iter().map(|&v| whole(v)).collect();
            for (token, logit) in (self.0)(step, &history) {
                row[token] = logit;
            }
            log_softmax(row);
        }
        Ok(())
    }
}

struct Deterministic(fn(usize, usize) -> usize);

impl Transducer for Deterministic {
    fn start(&self) -> Result<PredState, EngineError> {
        Ok(PredState { joint_pred: vec![0.0; HIDDEN], hidden: vec![0.0], cell: Vec::new() })
    }

    fn advance(&self, state: &PredState, token: usize) -> Result<PredState, EngineError> {
        let heard = state.hidden[0] + if token == BLANK { 0.0 } else { 1.0 };
        Ok(PredState { joint_pred: vec![0.0; HIDDEN], hidden: vec![heard], cell: Vec::new() })
    }

    fn project_encoder(&self, step: &[f32]) -> Result<Vec<f32>, EngineError> {
        let mut proj = vec![0.0; HIDDEN];
        proj[0] = step.first().copied().unwrap_or_default();
        Ok(proj)
    }

    fn log_probs(&self, enc_proj: &[f32], states: &[&PredState], out: &mut [f32]) -> Result<(), EngineError> {
        let step = whole(enc_proj[0]);
        for (state, row) in states.iter().zip(out.as_chunks_mut::<VOCAB_SIZE>().0) {
            row.fill(-30.0);
            let chosen = (self.0)(step, whole(state.hidden[0]));
            row[chosen] = 20.0;
            log_softmax(row);
        }
        Ok(())
    }
}

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

fn input<'a>(inputs: &'a [Input<'a>], wanted: &str) -> Option<&'a Input<'a>> {
    inputs.iter().find(|input| {
        let (Input::Floats { name, .. } | Input::Ints { name, .. }) = input;
        *name == wanted
    })
}

fn int(inputs: &[Input<'_>], wanted: &str) -> usize {
    match input(inputs, wanted) {
        Some(Input::Ints { values, .. }) => values.first().map_or(0, |&n| usize::try_from(n).unwrap_or(0)),
        Some(Input::Floats { .. }) | None => 0,
    }
}

struct Encoder {
    which: &'static str,
    expected_frames: usize,
    prompts: Prompts,
}

impl Part for Encoder {
    fn run(&self, inputs: &[Input<'_>], _: &[&str]) -> Result<Vec<Tensor>, EngineError> {
        shaped(inputs)?;
        let Some(Input::Floats { shape: [1, _, frames], .. }) = input(inputs, "mel") else {
            return Err(EngineError("no features".to_string()));
        };
        if *frames != self.expected_frames {
            return Err(EngineError(format!("mel has {frames} frames, expected {}", self.expected_frames)));
        }
        self.prompts.lock().push((self.which, int(inputs, "prompt_id")));
        let steps = frames.div_ceil(8);
        let encoder = Tensor { values: vec![0.0; ENCODED * steps], shape: vec![1, ENCODED, steps] };
        Ok(vec![encoder, count(int(inputs, "mel_length").div_ceil(8))])
    }
}
