//! Writing a file the service manager reads, which only an administrator can change.

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use anyhow::Context;

/// Writes `text` to `path`, readable by everyone and writable by its owner, by way of a name beside it, so the service
/// manager never reads half of it.
pub(super) fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    let name = path.file_name().context("naming the file")?.to_string_lossy();
    let partial = path.with_file_name(format!(".{name}.{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&partial)
        .with_context(|| format!("writing {}", partial.display()))?;
    file.write_all(text.as_bytes()).with_context(|| format!("writing {}", partial.display()))?;
    std::fs::rename(&partial, path).with_context(|| format!("moving {} to {}", partial.display(), path.display()))
}
