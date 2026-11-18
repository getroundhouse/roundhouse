//! Beam search across encoded steps, keeping alternative tokenizations alive until acoustic evidence and hint boundaries
//! settle them.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use super::super::language::OtherAlphabets;
use super::super::transducer::{BLANK, PredState, Transducer, VOCAB_SIZE};
use super::super::windows::Steps;
use super::Encoded;
use crate::EngineError;
use crate::hints::{HintGraph, HintState};

/// The most tokens one step can give, so a model that never says a step is done can't hang the loop.
pub(super) const MOST_A_STEP: usize = 10;

/// How many candidate transcripts beam search keeps per step.
const BEAM_WIDTH: usize = 4;

/// How close a token's log-probability must be to the step's likeliest choice (`e^-10 ≈ 1/22,000`) to enter a beam, so
/// hint boosts only tip acoustically plausible sounds rather than inventing words out of silence.
const ACOUSTIC_BEAM: f32 = 10.0;

/// Drop beams whose settled score has fallen more than `e^-8 ≈ 1/3,000` behind the best settled beam, so dead
/// branches don't hold back live streaming's agreed prefix.
const BEAM_FLOOR: f32 = 8.0;

/// The longest live streaming holds back an undecided word (~0.96 s) before committing the best settled beam.
pub(super) const MAX_LIVE_LAG: Steps = Steps(12);

/// A token heard, and the step it was heard in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Token {
    pub(in super::super) id: usize,
    pub(in super::super) at: Steps,
}

#[derive(Clone)]
struct Beam {
    tokens: Vec<Token>,
    state_id: u32,
    pred: Rc<PredState>,
    hint: HintState,
    score: f32,
}

struct Candidate {
    parent: usize,
    token: usize,
    hint: HintState,
    score: f32,
    settled: f32,
}

/// Carries active beams and cached prediction states across windows of a recording, which is never heard as a piece
/// in another alphabet than its language's.
pub(in super::super) struct Decoder<'a> {
    other_alphabets: &'a OtherAlphabets,
    beams: Vec<Beam>,
    cache: HashMap<(u32, usize), (u32, Rc<PredState>)>,
    next_state_id: u32,
    emitted: usize,
}

impl<'a> Decoder<'a> {
    pub(in super::super) fn new(other_alphabets: &'a OtherAlphabets) -> Self {
        Self { other_alphabets, beams: Vec::new(), cache: HashMap::new(), next_state_id: 1, emitted: 0 }
    }

    /// Decodes a whole piece from start to end, settling hint boundaries at the end and returning the winning tokens.
    pub(in super::super) fn decode_all(
        &mut self,
        transducer: &dyn Transducer,
        hints: &HintGraph,
        encoded: &Encoded,
        steps: Range<Steps>,
    ) -> Result<Vec<Token>, EngineError> {
        self.ensure_started(transducer)?;
        let saved = self.beams.clone();
        if let Err(err) = self.run_steps(transducer, hints, encoded, steps.clone()) {
            self.beams = saved;
            return Err(err);
        }
        for beam in &mut self.beams {
            beam.score += hints.finish(beam.hint);
            beam.hint = HintState::default();
        }
        self.beams.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(self.beams.first().map_or_else(Vec::new, |best| best.tokens.clone()))
    }

    /// Advances live decoding across `steps`, returning the newly agreed prefix of tokens.
    pub(in super::super) fn decode_live(
        &mut self,
        transducer: &dyn Transducer,
        hints: &HintGraph,
        encoded: &Encoded,
        steps: Range<Steps>,
    ) -> Result<Vec<Token>, EngineError> {
        self.ensure_started(transducer)?;
        let saved = self.beams.clone();
        let upto = steps.end;
        if let Err(err) = self.run_steps(transducer, hints, encoded, steps) {
            self.beams = saved;
            return Err(err);
        }
        Ok(self.commit_live_prefix(hints, upto))
    }

    /// Settles any undecided tail at the end of a live recording and emits the winning beam's remaining tokens.
    pub(in super::super) fn finish_live(&mut self, hints: &HintGraph) -> Vec<Token> {
        for beam in &mut self.beams {
            beam.score += hints.finish(beam.hint);
            beam.hint = HintState::default();
        }
        self.beams.sort_by(|a, b| b.score.total_cmp(&a.score));
        let Some(best) = self.beams.first().cloned() else { return Vec::new() };
        let rest = best.tokens.get(self.emitted..).map_or_else(Vec::new, <[Token]>::to_vec);
        self.emitted = best.tokens.len();
        self.beams = vec![best];
        rest
    }

    /// The earliest step where an active beam still holds an un-emitted token, if any.
    pub(in super::super) fn pending_from(&self) -> Option<Steps> {
        self.beams.iter().filter_map(|beam| beam.tokens.get(self.emitted).map(|t| t.at)).min()
    }

    fn ensure_started(&mut self, transducer: &dyn Transducer) -> Result<(), EngineError> {
        if self.beams.is_empty() {
            let start = Rc::new(transducer.start()?);
            self.beams.push(Beam {
                tokens: Vec::new(),
                state_id: 0,
                pred: start,
                hint: HintState::default(),
                score: 0.0,
            });
        }
        Ok(())
    }

    fn run_steps(
        &mut self,
        transducer: &dyn Transducer,
        hints: &HintGraph,
        encoded: &Encoded,
        steps: Range<Steps>,
    ) -> Result<(), EngineError> {
        let width = if hints.is_empty() { 1 } else { BEAM_WIDTH };
        for at in (steps.start.0..steps.end.0).map(Steps) {
            let step = encoded.step(at);
            let enc_proj = transducer.project_encoder(&step)?;
            let mut expanding = std::mem::take(&mut self.beams);
            let mut finished = Vec::with_capacity(width * 2);
            for round in 0..MOST_A_STEP {
                if expanding.is_empty() {
                    break;
                }
                expanding = self.expand_round(
                    transducer,
                    hints,
                    &enc_proj,
                    &expanding,
                    &mut finished,
                    at,
                    round + 1 == MOST_A_STEP,
                    width,
                )?;
            }
            self.beams = prune_beams(finished, hints, width);
        }
        Ok(())
    }

    #[expect(clippy::too_many_arguments, reason = "internal beam expansion step passing borrowed buffers")]
    fn expand_round(
        &mut self,
        transducer: &dyn Transducer,
        hints: &HintGraph,
        enc_proj: &[f32],
        expanding: &[Beam],
        finished: &mut Vec<Beam>,
        at: Steps,
        last_round: bool,
        width: usize,
    ) -> Result<Vec<Beam>, EngineError> {
        let states: Vec<&PredState> = expanding.iter().map(|b| &*b.pred).collect();
        let mut lps = vec![0.0_f32; states.len() * VOCAB_SIZE];
        transducer.log_probs(enc_proj, &states, &mut lps)?;
        // Before the cutoff below, which is then measured from the likeliest piece that's left.
        for row in lps.as_chunks_mut::<VOCAB_SIZE>().0 {
            self.other_alphabets.leave_out(row);
        }
        for (beam, row) in expanding.iter().zip(lps.as_chunks::<VOCAB_SIZE>().0) {
            finished.push(Beam { score: beam.score + row[BLANK], ..beam.clone() });
        }
        let best_finished =
            finished.iter().map(|b| b.score + hints.finish(b.hint)).reduce(f32::max).unwrap_or(f32::NEG_INFINITY);
        let floor = best_finished - ACOUSTIC_BEAM;
        let mut candidates = Vec::new();
        for (parent, (beam, row)) in expanding.iter().zip(lps.as_chunks::<VOCAB_SIZE>().0).enumerate() {
            let cutoff = row.iter().copied().fold(f32::NEG_INFINITY, f32::max) - ACOUSTIC_BEAM;
            for (token, &lp) in row[..BLANK].iter().enumerate() {
                if lp >= cutoff {
                    let (next_hint, delta) = hints.step(beam.hint, token);
                    let score = beam.score + lp + delta;
                    let settled = score + hints.finish(next_hint);
                    let keep =
                        if hints.is_empty() { score > best_finished } else { score >= floor || settled >= floor };
                    if keep {
                        candidates.push(Candidate { parent, token, hint: next_hint, score, settled });
                    }
                }
            }
        }
        let chosen = select_top_k(candidates, width, |c| c.score, |c| c.settled);
        let mut next_beams = Vec::with_capacity(chosen.len());
        for cand in chosen {
            let base = &expanding[cand.parent];
            let (state_id, pred) = self.advance_pred(transducer, base.state_id, &base.pred, cand.token)?;
            let mut tokens = base.tokens.clone();
            tokens.push(Token { id: cand.token, at });
            let beam = Beam { tokens, state_id, pred, hint: cand.hint, score: cand.score };
            if last_round {
                finished.push(beam);
            } else {
                next_beams.push(beam);
            }
        }
        Ok(next_beams)
    }

    fn advance_pred(
        &mut self,
        transducer: &dyn Transducer,
        state_id: u32,
        pred: &PredState,
        token: usize,
    ) -> Result<(u32, Rc<PredState>), EngineError> {
        if let Some(hit) = self.cache.get(&(state_id, token)) {
            return Ok(hit.clone());
        }
        let next_pred = Rc::new(transducer.advance(pred, token)?);
        let next_id = self.next_state_id;
        self.next_state_id = self.next_state_id.saturating_add(1);
        self.cache.insert((state_id, token), (next_id, next_pred.clone()));
        Ok((next_id, next_pred))
    }

    fn commit_live_prefix(&mut self, hints: &HintGraph, upto: Steps) -> Vec<Token> {
        if self.beams.is_empty() {
            return Vec::new();
        }
        if let Some(earliest) = self.pending_from()
            && upto.0.saturating_sub(earliest.0) > MAX_LIVE_LAG.0
            && let Some(best_idx) = self
                .beams
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| (a.score + hints.finish(a.hint)).total_cmp(&(b.score + hints.finish(b.hint))))
                .map(|(i, _)| i)
        {
            let cutoff = Steps(upto.0 - MAX_LIVE_LAG.0);
            let commit_len = self.beams[best_idx].tokens.partition_point(|t| t.at < cutoff).max(self.emitted);
            let prefix: Vec<usize> = self.beams[best_idx].tokens[..commit_len].iter().map(|t| t.id).collect();
            self.beams.retain(|b| {
                b.tokens.len() >= commit_len
                    && b.tokens[..commit_len].iter().map(|t| t.id).eq(prefix.iter().copied())
                    && b.tokens.get(commit_len).is_none_or(|t| t.at >= cutoff)
            });
        }
        let agreed = common_prefix_len(&self.beams, self.emitted);
        let out = self.beams[0].tokens[self.emitted..agreed].to_vec();
        self.emitted = agreed;
        out
    }
}

fn common_prefix_len(beams: &[Beam], from: usize) -> usize {
    let Some(first) = beams.first() else { return from };
    let max_len = beams.iter().map(|b| b.tokens.len()).min().unwrap_or(from);
    (from..max_len)
        .take_while(|&i| beams[1..].iter().all(|b| b.tokens[i].id == first.tokens[i].id))
        .last()
        .map_or(from, |last| last + 1)
}

fn select_top_k<T>(mut items: Vec<T>, width: usize, score: impl Fn(&T) -> f32, settled: impl Fn(&T) -> f32) -> Vec<T> {
    if items.len() <= width {
        items.sort_by(|a, b| score(b).total_cmp(&score(a)));
        return items;
    }
    let best_settled =
        items.iter().enumerate().max_by(|(_, a), (_, b)| settled(a).total_cmp(&settled(b))).map(|(idx, _)| idx);
    let protected = best_settled.map(|idx| items.swap_remove(idx));
    items.sort_by(|a, b| score(b).total_cmp(&score(a)));
    let mut out = Vec::with_capacity(width);
    out.extend(protected);
    out.extend(items.into_iter().take(width.saturating_sub(out.len())));
    out.sort_by(|a, b| score(b).total_cmp(&score(a)));
    out
}

fn prune_beams(beams: Vec<Beam>, hints: &HintGraph, width: usize) -> Vec<Beam> {
    let mut deduped: Vec<Beam> = Vec::with_capacity(beams.len());
    for beam in beams {
        if let Some(existing) = deduped.iter_mut().find(|b| {
            b.state_id == beam.state_id
                && b.tokens.len() == beam.tokens.len()
                && b.tokens.iter().zip(&beam.tokens).all(|(x, y)| x.id == y.id)
        }) {
            if beam.score > existing.score {
                *existing = beam;
            }
        } else {
            deduped.push(beam);
        }
    }
    if let Some(best_settled_score) = deduped.iter().map(|b| b.score + hints.finish(b.hint)).reduce(f32::max) {
        let floor = best_settled_score - BEAM_FLOOR;
        deduped.retain(|b| b.score >= floor || b.score + hints.finish(b.hint) >= floor);
    }
    select_top_k(deduped, width, |b| b.score, |b| b.score + hints.finish(b.hint))
}
