//! The clips each set is made of, from its dataset at a pinned revision, and how a run picks some of them.
//!
//! Only clips' names and counts go in the repository, never what's said in them, so the datasets' texts stay with
//! the datasets.

mod choosing;
mod librispeech;
mod tech_sentences;

pub(crate) use choosing::{AMONG, clips, spread, unrelated};
pub(crate) use librispeech::LIBRISPEECH;
pub(crate) use tech_sentences::{SoundAlike, TECH_SENTENCES};
