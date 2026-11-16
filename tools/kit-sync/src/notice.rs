//! The line each file we change carries, as Apache-2.0 asks a changed file to say it was changed.

use std::fs;
use std::path::Path;

use crate::error::{At, Error};
use crate::files::{CopyPath, files_under};

/// The words of the line, which `third_party/README.md` says each changed file holds in a comment at its top.
pub const NOTICE: &str = "Changed in Roundhouse's copy of gpui-kit, as third_party/README.md lists.";

/// `text` without the lines that hold [`NOTICE`], or `None` when it has none, since a patch offered upstream has none.
pub(crate) fn without_notices(text: &[u8]) -> Option<Vec<u8>> {
    let noticed = |line: &[u8]| line.windows(NOTICE.len()).any(|words| words == NOTICE.as_bytes());
    if !noticed(text) {
        return None;
    }
    Some(text.split_inclusive(|byte| *byte == b'\n').filter(|line| !noticed(line)).flatten().copied().collect())
}

/// Puts [`NOTICE`] at the top of each `.rs` and `.toml` file in `dir` that differs from `base`, and leaves it off the
/// rest.
pub(crate) fn stamp_notices(dir: &Path, base: &Path) -> Result<(), Error> {
    for path in files_under(dir)? {
        let Some(prefix) = comment_prefix(&path) else { continue };
        let file = path.under(dir);
        if !fs::symlink_metadata(&file).at(&file)?.is_file() {
            continue;
        }
        let raw = fs::read(&file).at(&file)?;
        let clean = without_notices(&raw).unwrap_or_else(|| raw.clone());
        let expected = if changed_from_base(&path, &clean, base)? {
            let mut stamped = format!("{prefix}{NOTICE}\n").into_bytes();
            stamped.extend_from_slice(&clean);
            stamped
        } else {
            clean
        };
        if raw != expected {
            fs::write(&file, expected).at(&file)?;
        }
    }
    Ok(())
}

/// Which of `files` in `copy` don't carry [`NOTICE`] where they differ from `base`, or carry it where they don't.
pub(crate) fn wrong_notices(copy: &Path, files: &[CopyPath], base: &Path) -> Result<Vec<CopyPath>, Error> {
    let mut wrong = Vec::new();
    for path in files {
        let Some(prefix) = comment_prefix(path) else { continue };
        let file = path.under(copy);
        if !fs::symlink_metadata(&file).at(&file)?.is_file() {
            continue;
        }
        let raw = fs::read(&file).at(&file)?;
        let clean = without_notices(&raw).unwrap_or_else(|| raw.clone());
        let ok = if changed_from_base(path, &clean, base)? {
            let line = format!("{prefix}{NOTICE}\n");
            raw.starts_with(line.as_bytes()) && &raw[line.len()..] == clean.as_slice()
        } else {
            raw == clean
        };
        if !ok {
            wrong.push(path.clone());
        }
    }
    Ok(wrong)
}

fn comment_prefix(path: &CopyPath) -> Option<&'static str> {
    match Path::new(path.as_str()).extension().and_then(|ext| ext.to_str()) {
        Some("rs") => Some("// "),
        Some("toml") => Some("# "),
        _ => None,
    }
}

fn changed_from_base(path: &CopyPath, clean: &[u8], base: &Path) -> Result<bool, Error> {
    if path.as_str() == "Cargo.toml" {
        return Ok(true);
    }
    let base_file = path.under(base);
    if !fs::symlink_metadata(&base_file).is_ok_and(|meta| meta.is_file()) {
        return Ok(true);
    }
    Ok(fs::read(&base_file).at(&base_file)? != clean)
}
