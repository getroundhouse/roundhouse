//! Greedy decoding of what an encoder made out, a step at a time: the joint gives the likeliest next token, from a step
//! and the tokens so far, or says the step has no more, and the decoder carries the tokens so far. Ported from
//! `FluidAudio`'s `UnifiedRnntDecoder`.

use std::ops::Range;

use super::parts::{Input, Parts, Tensor};
use super::windows::Steps;
use crate::EngineError;

/// The token that says a step has no more.
pub(super) const BLANK: usize = 4095;

/// The decoder's state: two layers of 640.
const STATE: [usize; 3] = [2, 1, 640];
const STATE_VALUES: usize = STATE[0] * STATE[1] * STATE[2];

/// What the decoder gives for the tokens so far.
pub(super) const SAID: usize = 640;

/// How many values the encoders give each step.
pub(super) const ENCODED: usize = 1024;

/// The most tokens one step can give, so a model that never says a step is done can't hang the loop.
pub(super) const MOST_A_STEP: usize = 10;

/// What an encoder made out of a window.
pub(super) struct Encoded {
    /// `ENCODED` rows of `steps` values.
    values: Vec<f32>,
    steps: usize,
    /// Where the window starts in the recording.
    first: Steps,
    /// How many of its steps are audio, from the first.
    audio: Steps,
}

impl Encoded {
    /// The encoder's `encoder` and `encoder_length` outputs, for a window that starts at `first` in the recording.
    pub(super) fn new(encoder: Tensor, length: &Tensor, first: Steps) -> Result<Self, EngineError> {
        let steps = match encoder.shape.as_slice() {
            [1, values, steps] if *values == ENCODED && encoder.values.len() == ENCODED * steps => *steps,
            shape => return Err(EngineError(format!("an encoder gave a shape of {shape:?}"))),
        };
        Ok(Self { values: encoder.values, steps, first, audio: Steps(length.whole()?.min(steps)) })
    }

    /// Where what it made out of the window's audio ends in the recording.
    pub(super) fn made_out(&self) -> Steps {
        Steps(self.first.0 + self.audio.0)
    }

    /// What it made out of the step `at`, a place in the recording within the window's audio.
    fn step(&self, at: Steps) -> Vec<f32> {
        let step = at.0 - self.first.0;
        (0..ENCODED).map(|value| self.values[value * self.steps + step]).collect()
    }
}

/// A token heard, and the step it was heard in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) id: usize,
    pub(super) at: Steps,
}

/// The decoder, as it stands after the tokens so far.
pub(super) struct Decoder {
    /// Its output and state after the last token, once it has heard one, or the start.
    said: Option<Said>,
}

struct Said {
    out: Vec<f32>,
    hidden: Vec<f32>,
    cell: Vec<f32>,
}

impl Decoder {
    pub(super) fn new() -> Self {
        Self { said: None }
    }

    /// The tokens in `steps` of `encoded`, places in the recording. On an error, the decoder stays as it was after
    /// the last token it heard.
    pub(super) fn decode(
        &mut self,
        parts: &Parts,
        encoded: &Encoded,
        steps: Range<Steps>,
    ) -> Result<Vec<Token>, EngineError> {
        let mut said = match self.said.take() {
            Some(said) => said,
            None => hear(parts, BLANK, &[0.0; STATE_VALUES], &[0.0; STATE_VALUES])?,
        };
        let mut tokens = Vec::new();
        let decoded = decode_steps(parts, encoded, steps, &mut said, &mut tokens);
        self.said = Some(said);
        decoded.map(|()| tokens)
    }
}

fn decode_steps(
    parts: &Parts,
    encoded: &Encoded,
    steps: Range<Steps>,
    said: &mut Said,
    tokens: &mut Vec<Token>,
) -> Result<(), EngineError> {
    for at in (steps.start.0..steps.end.0).map(Steps) {
        let step = encoded.step(at);
        for _ in 0..MOST_A_STEP {
            let id = next(parts, &step, &said.out)?;
            if id == BLANK {
                break;
            }
            *said = hear(parts, id, &said.hidden, &said.cell)?;
            tokens.push(Token { id, at });
        }
    }
    Ok(())
}

/// The decoder, after it has heard `token`, from the state it had.
fn hear(parts: &Parts, token: usize, hidden: &[f32], cell: &[f32]) -> Result<Said, EngineError> {
    let token = i32::try_from(token).map_err(|_| EngineError(format!("no such token as {token}")))?;
    let inputs = [
        Input::Ints { name: "targets", values: &[token], shape: &[1, 1] },
        Input::Ints { name: "target_length", values: &[1], shape: &[1] },
        Input::Floats { name: "h_in", values: hidden, shape: &STATE },
        Input::Floats { name: "c_in", values: cell, shape: &STATE },
    ];
    let outputs = parts.decoder.run(&inputs, &["decoder", "h_out", "c_out"])?;
    let [out, hidden, cell] = <[Tensor; 3]>::try_from(outputs).map_err(|_| missing("the decoder"))?;
    let (out, hidden, cell) = (out.values, hidden.values, cell.values);
    if out.len() != SAID || hidden.len() != STATE_VALUES || cell.len() != STATE_VALUES {
        return Err(EngineError("the decoder gave the wrong number of values".to_string()));
    }
    Ok(Said { out, hidden, cell })
}

/// The joint's likeliest token after `said`, in the step `step`, or `BLANK`.
fn next(parts: &Parts, step: &[f32], said: &[f32]) -> Result<usize, EngineError> {
    let inputs = [
        Input::Floats { name: "encoder_step", values: step, shape: &[1, ENCODED, 1] },
        Input::Floats { name: "decoder_step", values: said, shape: &[1, SAID, 1] },
    ];
    let outputs = parts.joint.run(&inputs, &["token_id"])?;
    outputs.first().ok_or_else(|| missing("the joint"))?.whole()
}

fn missing(part: &str) -> EngineError {
    EngineError(format!("{part} didn't give all it should"))
}

#[cfg(test)]
mod tests {
    use super::{Decoder, ENCODED, Encoded, MOST_A_STEP, Token};
    use crate::nemotron::parts::Tensor;
    use crate::nemotron::stand_in;
    use crate::nemotron::windows::Steps;

    /// What an encoder made out of a window at `first`, each step's values all the step's own.
    fn encoded(steps: &[f32], first: Steps) -> Encoded {
        let values = (0..ENCODED).flat_map(|_| steps.iter().copied()).collect();
        let encoder = Tensor { values, shape: vec![1, ENCODED, steps.len()] };
        Encoded::new(encoder, &stand_in::count(steps.len()), first).unwrap()
    }

    fn ids(tokens: &[Token]) -> Vec<(usize, usize)> {
        tokens.iter().map(|token| (token.id, token.at.0)).collect()
    }

    #[test]
    fn decoding_a_window_in_two_goes_on_from_the_tokens_so_far_as_decoding_it_whole_does() {
        // Each step says how many tokens there should be by its end.
        let parts = stand_in::counting_parts();
        let window = encoded(&[1.0, 1.0, 3.0, 3.0, 4.0, 6.0], Steps(10));

        let whole = Decoder::new().decode(&parts, &window, Steps(10)..Steps(16)).unwrap();
        let mut halves = Decoder::new();
        let mut split = halves.decode(&parts, &window, Steps(10)..Steps(13)).unwrap();
        split.extend(halves.decode(&parts, &window, Steps(13)..Steps(16)).unwrap());

        assert_eq!(ids(&whole), [(1, 10), (2, 12), (3, 12), (4, 14), (5, 15), (6, 15)]);
        assert_eq!(ids(&split), ids(&whole));
    }

    #[test]
    fn a_step_gives_at_most_ten_tokens_and_the_next_is_decoded() {
        let parts = stand_in::never_done_parts();
        let window = encoded(&[0.0, 0.0], Steps(0));

        let tokens = Decoder::new().decode(&parts, &window, Steps(0)..Steps(2)).unwrap();

        assert_eq!(tokens.len(), 2 * MOST_A_STEP);
        assert!(tokens[..MOST_A_STEP].iter().all(|token| token.at == Steps(0)));
        assert!(tokens[MOST_A_STEP..].iter().all(|token| token.at == Steps(1)));
    }

    #[test]
    fn a_step_is_read_down_its_column_and_an_encoder_of_another_shape_is_refused() {
        // Value `v` of step `s` is `100 s + v`, laid out a row for each value.
        let steps = 3;
        let values = (0..ENCODED).flat_map(|v| (0..steps).map(move |s| stand_in::real(100 * s + v))).collect();
        let encoder = Tensor { values, shape: vec![1, ENCODED, steps] };
        let window = Encoded::new(encoder, &stand_in::count(steps), Steps(7)).unwrap();

        let step = window.step(Steps(8));
        assert_eq!((step[0], step[5]), (100.0, 105.0));
        assert_eq!(window.made_out(), Steps(10));
        let values = vec![0.0; ENCODED * steps];
        let longer = Encoded::new(Tensor { values, shape: vec![1, ENCODED, steps] }, &stand_in::count(9), Steps(7));
        assert_eq!(longer.unwrap().made_out(), Steps(10), "no more steps than it gave");

        let wrong = Tensor { values: vec![0.0; ENCODED * steps], shape: vec![1, steps, ENCODED] };
        assert!(Encoded::new(wrong, &stand_in::count(steps), Steps(0)).is_err());
    }
}
