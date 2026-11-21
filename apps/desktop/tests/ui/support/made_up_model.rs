//! A model for dictation, made up for a test: files of a pattern, with their sizes and SHA-256, for a server on this
//! computer to serve as Hugging Face would, or for a test to put where the model is kept.

use std::path::Path;

use models::{ModelFile, ModelFiles};
use ring::digest::{SHA256, digest};

/// A model of a file at each of `paths`, of `bytes` each, and what's in each.
pub(crate) fn made_up(paths: &[&'static str], bytes: usize) -> (ModelFiles, Vec<Vec<u8>>) {
    let body: Vec<u8> = (0..bytes).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let sha256: [u8; 32] = digest(&SHA256, &body).as_ref().try_into().unwrap();
    let files: Vec<ModelFile> =
        paths.iter().map(|&path| ModelFile { path, size_bytes: bytes as u64, sha256 }).collect();
    let files = ModelFiles { repository: "test", revision: "made-up", files: Box::leak(files.into_boxed_slice()) };
    (files, vec![body; paths.len()])
}

/// Puts `body` in place as `file` of `files`, under `models`, as a download that finished would.
pub(crate) fn put(models: &Path, files: &ModelFiles, file: &ModelFile, body: &[u8]) {
    let at = files.dir(models).join(file.path);
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, body).unwrap();
}
