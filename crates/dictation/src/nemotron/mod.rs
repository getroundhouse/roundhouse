//! The engine for NVIDIA's nemotron-labs-unified-asr-0.6b, a model that hears speech both live and whole, run on
//! Core ML from Fluid Inference's export of it. Its live encoder hears 160 ms of new audio at a time, with 5.6 s
//! before it and 0.4 s after, so a word shows about 0.4 s after it's said. Its offline encoder hears a piece of up to
//! 15 s whole, for the piece's final text. A decoder and a joint turn what either made out into tokens, and the
//! vocabulary turns them into text.

#[cfg(target_os = "macos")]
mod coreml;
mod decoding;
mod engine;
mod files;
mod live;
mod mel;
mod parts;
#[cfg(test)]
mod stand_in;
mod vocab;
mod windows;

pub use engine::{Nemotron, OLDEST_MACOS, runs_here};
pub use files::MODEL;
