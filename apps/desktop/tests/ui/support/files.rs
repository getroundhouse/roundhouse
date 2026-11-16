//! Looking for a token on disk, to see that only the hub keeps it.

#![cfg(test)]

use std::path::{Path, PathBuf};

/// Every file under `dir` that holds `text`, but for what's under a directory named `except`.
pub(crate) fn files_holding(dir: &Path, text: &str, except: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() != except {
                found.extend(files_holding(&path, text, except));
            }
        } else if std::fs::read(&path).is_ok_and(|bytes| bytes.windows(text.len()).any(|at| at == text.as_bytes())) {
            found.push(path);
        }
    }
    found
}
