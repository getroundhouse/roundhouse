//! Running the model's prediction network (embedding and two-layer LSTM) and joint network directly in Rust, from the
//! `fp16` weights inside the Core ML bundles on disk, so beam search sees all 4,096 token log-probabilities with no
//! per-step Core ML call overhead.

use std::path::Path;

use super::blob::MlModelWeights;
use super::linear::linear_rows;
use super::parts::{DECODER, JOINT};
use crate::EngineError;

pub(super) const VOCAB_SIZE: usize = 4096;
pub(super) const BLANK: usize = 4095;
pub(super) const ENCODED: usize = 1024;
pub(super) const HIDDEN: usize = 640;
const GATES: usize = 4 * HIDDEN;
const ZERO_GATES: [f32; GATES] = [0.0; GATES];

/// What the prediction network carries after the tokens emitted so far, plus its cached joint projection.
#[derive(Clone)]
pub(super) struct PredState {
    pub(super) joint_pred: Vec<f32>,
    pub(super) hidden: Vec<f32>,
    pub(super) cell: Vec<f32>,
}

/// Scores candidate tokens from an encoded audio step and a prediction state: the real weights, or a stand-in in tests.
pub(super) trait Transducer {
    fn start(&self) -> Result<PredState, EngineError>;
    fn advance(&self, state: &PredState, token: usize) -> Result<PredState, EngineError>;
    fn project_encoder(&self, step: &[f32]) -> Result<Vec<f32>, EngineError>;
    fn log_probs(&self, enc_proj: &[f32], states: &[&PredState], out: &mut [f32]) -> Result<(), EngineError>;
}

struct LstmLayer {
    weight_ih: Vec<f32>,
    weight_hh: Vec<f32>,
    bias: Vec<f32>,
}

pub(super) struct RustTransducer {
    embed: Vec<f32>,
    layer0: LstmLayer,
    layer1: LstmLayer,
    enc_weight: Vec<f32>,
    enc_bias: Vec<f32>,
    pred_weight: Vec<f32>,
    pred_bias: Vec<f32>,
    out_weight: Vec<f32>,
    out_bias: Vec<f32>,
    initial: PredState,
}

impl RustTransducer {
    pub(super) fn load(dir: &Path) -> Result<Self, EngineError> {
        let dec = MlModelWeights::read(&dir.join(DECODER))?;
        let embed = dec.tensor("module_prediction_embed_weight_to_fp16", VOCAB_SIZE * HIDDEN)?;
        let layer0 = LstmLayer {
            weight_ih: dec.tensor("concat_1_to_fp16", GATES * HIDDEN)?,
            weight_hh: dec.tensor("concat_2_to_fp16", GATES * HIDDEN)?,
            bias: dec.tensor("concat_0_to_fp16", GATES)?,
        };
        let layer1 = LstmLayer {
            weight_ih: dec.tensor("concat_4_to_fp16", GATES * HIDDEN)?,
            weight_hh: dec.tensor("concat_5_to_fp16", GATES * HIDDEN)?,
            bias: dec.tensor("concat_3_to_fp16", GATES)?,
        };
        let joint = MlModelWeights::read(&dir.join(JOINT))?;
        let enc_weight = joint.tensor("joint_module_enc_weight_to_fp16", HIDDEN * ENCODED)?;
        let enc_bias = joint.tensor("joint_module_enc_bias_to_fp16", HIDDEN)?;
        let pred_weight = joint.tensor("joint_module_pred_weight_to_fp16", HIDDEN * HIDDEN)?;
        let pred_bias = joint.tensor("joint_module_pred_bias_to_fp16", HIDDEN)?;
        let out_weight = joint.tensor("joint_module_joint_net_2_weight_to_fp16", VOCAB_SIZE * HIDDEN)?;
        let out_bias = joint.tensor("joint_module_joint_net_2_bias_to_fp16", VOCAB_SIZE)?;
        let zero =
            PredState { joint_pred: vec![0.0; HIDDEN], hidden: vec![0.0; 2 * HIDDEN], cell: vec![0.0; 2 * HIDDEN] };
        let mut this = Self {
            embed,
            layer0,
            layer1,
            enc_weight,
            enc_bias,
            pred_weight,
            pred_bias,
            out_weight,
            out_bias,
            initial: zero.clone(),
        };
        this.initial = this.step_from(&zero, BLANK)?;
        Ok(this)
    }

    fn step_from(&self, state: &PredState, token: usize) -> Result<PredState, EngineError> {
        if token >= VOCAB_SIZE {
            return Err(EngineError(format!("no such token as {token}")));
        }
        let x0 = &self.embed[token * HIDDEN..(token + 1) * HIDDEN];
        let mut hidden = vec![0.0; 2 * HIDDEN];
        let mut cell = vec![0.0; 2 * HIDDEN];
        lstm_step(
            x0,
            &state.hidden[..HIDDEN],
            &state.cell[..HIDDEN],
            &self.layer0,
            &mut hidden[..HIDDEN],
            &mut cell[..HIDDEN],
        );
        let h0: Vec<f32> = hidden[..HIDDEN].to_vec();
        lstm_step(
            &h0,
            &state.hidden[HIDDEN..],
            &state.cell[HIDDEN..],
            &self.layer1,
            &mut hidden[HIDDEN..],
            &mut cell[HIDDEN..],
        );
        let mut joint_pred = vec![0.0; HIDDEN];
        linear_rows(&hidden[HIDDEN..], 1, &self.pred_weight, &self.pred_bias, &mut joint_pred);
        Ok(PredState { joint_pred, hidden, cell })
    }
}

impl Transducer for RustTransducer {
    fn start(&self) -> Result<PredState, EngineError> {
        Ok(self.initial.clone())
    }

    fn advance(&self, state: &PredState, token: usize) -> Result<PredState, EngineError> {
        self.step_from(state, token)
    }

    fn project_encoder(&self, step: &[f32]) -> Result<Vec<f32>, EngineError> {
        if step.len() != ENCODED {
            return Err(EngineError(format!("encoder step has {} values, expected {ENCODED}", step.len())));
        }
        let mut proj = vec![0.0; HIDDEN];
        linear_rows(step, 1, &self.enc_weight, &self.enc_bias, &mut proj);
        Ok(proj)
    }

    fn log_probs(&self, enc_proj: &[f32], states: &[&PredState], out: &mut [f32]) -> Result<(), EngineError> {
        if enc_proj.len() != HIDDEN || out.len() != states.len() * VOCAB_SIZE {
            return Err(EngineError("joint inputs aren't the right shape".to_string()));
        }
        if states.is_empty() {
            return Ok(());
        }
        let mut joint_hidden = vec![0.0; states.len() * HIDDEN];
        for (row, state) in joint_hidden.as_chunks_mut::<HIDDEN>().0.iter_mut().zip(states.iter()) {
            for ((dst, &e), &p) in row.iter_mut().zip(enc_proj.iter()).zip(state.joint_pred.iter()) {
                *dst = (e + p).max(0.0);
            }
        }
        linear_rows(&joint_hidden, states.len(), &self.out_weight, &self.out_bias, out);
        for row in out.as_chunks_mut::<VOCAB_SIZE>().0 {
            log_softmax(row);
        }
        Ok(())
    }
}

/// Core ML MIL `lstm` orders its four gates `[input, forget, output, cell]` (`ifoz`).
fn lstm_step(input: &[f32], h_prev: &[f32], c_prev: &[f32], layer: &LstmLayer, h_out: &mut [f32], c_out: &mut [f32]) {
    let mut gates = [0.0_f32; GATES];
    let mut hh = [0.0_f32; GATES];
    linear_rows(input, 1, &layer.weight_ih, &layer.bias, &mut gates);
    linear_rows(h_prev, 1, &layer.weight_hh, &ZERO_GATES, &mut hh);
    for (gate, &rec) in gates.iter_mut().zip(hh.iter()) {
        *gate += rec;
    }
    for idx in 0..HIDDEN {
        let in_gate = sigmoid(gates[idx]);
        let forget_gate = sigmoid(gates[HIDDEN + idx]);
        let out_gate = sigmoid(gates[2 * HIDDEN + idx]);
        let cell_gate = gates[3 * HIDDEN + idx].tanh();
        let next_cell = forget_gate * c_prev[idx] + in_gate * cell_gate;
        c_out[idx] = next_cell;
        h_out[idx] = out_gate * next_cell.tanh();
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

pub(super) fn log_softmax(values: &mut [f32]) {
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum_exp: f32 = values.iter().map(|&v| (v - max).exp()).sum();
    let shift = max + sum_exp.ln();
    for v in values {
        *v -= shift;
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::path::Path;

    use super::{BLANK, DECODER, ENCODED, GATES, HIDDEN, JOINT, RustTransducer, Transducer as _, VOCAB_SIZE};
    use crate::nemotron::blob::{MODEL_MIL, WEIGHT_BIN};

    fn write_bundle(dir: &Path, tensors: &[(&str, usize, u16)]) {
        std::fs::create_dir_all(dir.join("weights")).unwrap();
        let mut mil = String::new();
        let mut bin = vec![0u8; 64];
        for &(name, count, f16_val) in tensors {
            let offset = bin.len();
            let _ = writeln!(
                mil,
                "tensor<fp16, [{count}]> {name} = const()[val = tensor<fp16, [{count}]>(BLOBFILE(path = tensor<string, []>(\"@model_path/weights/weight.bin\"), offset = tensor<uint64, []>({offset})))];"
            );
            let mut header = [0u8; 64];
            header[..4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
            header[4..8].copy_from_slice(&1u32.to_le_bytes());
            let byte_len = count * 2;
            header[8..16].copy_from_slice(&u64::try_from(byte_len).unwrap().to_le_bytes());
            header[16..24].copy_from_slice(&u64::try_from(offset + 64).unwrap().to_le_bytes());
            bin.extend_from_slice(&header);
            for _ in 0..count {
                bin.extend_from_slice(&f16_val.to_le_bytes());
            }
            let pad = (64 - (bin.len() % 64)) % 64;
            bin.resize(bin.len() + pad, 0);
        }
        std::fs::write(dir.join(MODEL_MIL), mil).unwrap();
        std::fs::write(dir.join(WEIGHT_BIN), bin).unwrap();
    }

    #[test]
    fn loads_weights_runs_ifoz_lstm_and_computes_normalized_log_probs() {
        let dir = tempfile::tempdir().unwrap();
        write_bundle(
            &dir.path().join(DECODER),
            &[
                ("module_prediction_embed_weight_to_fp16", VOCAB_SIZE * HIDDEN, 0x0000),
                ("concat_1_to_fp16", GATES * HIDDEN, 0x0000),
                ("concat_2_to_fp16", GATES * HIDDEN, 0x0000),
                ("concat_0_to_fp16", GATES, 0x0000),
                ("concat_4_to_fp16", GATES * HIDDEN, 0x0000),
                ("concat_5_to_fp16", GATES * HIDDEN, 0x0000),
                ("concat_3_to_fp16", GATES, 0x0000),
            ],
        );
        write_bundle(
            &dir.path().join(JOINT),
            &[
                ("joint_module_enc_weight_to_fp16", HIDDEN * ENCODED, 0x0000),
                ("joint_module_enc_bias_to_fp16", HIDDEN, 0x3c00), // 1.0
                ("joint_module_pred_weight_to_fp16", HIDDEN * HIDDEN, 0x0000),
                ("joint_module_pred_bias_to_fp16", HIDDEN, 0x0000),
                ("joint_module_joint_net_2_weight_to_fp16", VOCAB_SIZE * HIDDEN, 0x0000),
                ("joint_module_joint_net_2_bias_to_fp16", VOCAB_SIZE, 0x0000),
            ],
        );

        let transducer = RustTransducer::load(dir.path()).unwrap();
        let s0 = transducer.start().unwrap();
        assert!(s0.hidden.iter().all(|&v| v.abs() < 1e-6));
        let s1 = transducer.advance(&s0, 1).unwrap();
        assert!(transducer.advance(&s0, BLANK + 1).is_err());
        let enc = transducer.project_encoder(&vec![0.0; ENCODED]).unwrap();
        let mut out = vec![0.0; 2 * VOCAB_SIZE];
        transducer.log_probs(&enc, &[&s0, &s1], &mut out).unwrap();
        let prob_sum: f32 = out[..VOCAB_SIZE].iter().map(|&lp| lp.exp()).sum();
        assert!((prob_sum - 1.0).abs() < 1e-4, "sum={prob_sum}");
    }
}
