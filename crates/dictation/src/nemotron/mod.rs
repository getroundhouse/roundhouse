//! The engine for NVIDIA's nemotron-labs-unified-asr-0.6b, a model that hears speech both live and whole. Its live and
//! offline encoders run on Core ML from Fluid Inference's export: the live encoder hears 160 ms of new audio at a time,
//! with 5.6 s before it and 0.4 s after, so a word shows about 0.4 s after it's said, and the offline encoder hears a
//! piece of up to 15 s whole, for the piece's final text. Its prediction and joint networks run in Rust from the same
//! bundle's weights, so beam search can bias toward hint words over all 4,096 token probabilities and take back partial
//! matches when what follows diverges.

mod blob;
#[cfg(target_os = "macos")]
mod coreml;
mod decoding;
mod engine;
mod files;
mod linear;
mod live;
mod mel;
mod parts;
#[cfg(test)]
mod stand_in;
mod transducer;
mod vocab;
mod windows;

pub use engine::{Nemotron, OLDEST_MACOS, runs_here};
pub use files::MODEL;
