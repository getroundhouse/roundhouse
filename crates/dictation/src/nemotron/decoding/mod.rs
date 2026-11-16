//! RNN-T beam search across encoded steps over [`Transducer`] and [`HintGraph`]: scores candidate subword paths across
//! each encoded audio step, protects the best unboosted acoustic path while hint prefixes are still provisional, and
//! emits only the prefix every active beam agrees on during live streaming.

mod beam;
mod encoded;

pub(super) use beam::{Decoder, Token};
#[cfg(test)]
use beam::{MAX_LIVE_LAG, MOST_A_STEP};
pub(super) use encoded::Encoded;

#[cfg(test)]
mod tests;
