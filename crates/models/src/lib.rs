//! A model's files on this device: which they are, each with its size and SHA-256, where they're kept, whether they're
//! all there, and, with the `download` feature, downloading them from Hugging Face and deleting them. Every revision
//! of a model is kept in a folder of its own under one folder for that model alone, `model_dir`, which the app
//! chooses: downloading removes the other revisions in it, and deleting removes everything. It knows nothing of what a
//! model does, so whatever runs one, such as dictation, keeps its files with this. No UI and no async runtime, as
//! `docs/principles/layers.md` says.

#![warn(clippy::wildcard_enum_match_arm)]

#[cfg(feature = "download")]
mod download;
mod files;
#[cfg(test)]
mod tests;

#[cfg(feature = "download")]
pub use download::{DownloadError, FolderError, Progress, delete};
#[cfg(feature = "download")]
pub use fetching::{CheckError, FetchError, Fetched};
pub use files::{ModelFile, ModelFiles, bytes_on_disk};
