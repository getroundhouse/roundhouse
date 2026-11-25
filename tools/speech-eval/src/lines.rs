//! A run's `heard.jsonl`: a line for each thing heard, as it's heard, so a run stopped partway goes on from there.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::Serialize;
use serde::de::DeserializeOwned;

fn path(dir: &Path) -> PathBuf {
    dir.join("heard.jsonl")
}

/// What's been heard in the run in `dir`. A line is whole once its newline is written, so one without was cut short as a
/// run stopped, and is left out, to be heard again.
pub(crate) fn read<T: DeserializeOwned>(dir: &Path) -> anyhow::Result<Vec<T>> {
    let path = path(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("couldn't read {}", path.display())),
    };
    let whole = text.rfind('\n').map_or("", |end| &text[..end]);
    let parse = |(i, line)| {
        serde_json::from_str(line).with_context(|| format!("{}:{} isn't what it should be", path.display(), i + 1))
    };
    whole.lines().enumerate().map(parse).collect()
}

/// Adds `heard`, written at once, and in one write, so a run that stops keeps it, or a line cut short.
pub(crate) fn add<T: Serialize>(dir: &Path, heard: &T) -> anyhow::Result<()> {
    let path = path(dir);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(heard)? + "\n";
    file.write_all(line.as_bytes()).with_context(|| format!("couldn't write {}", path.display()))
}

/// Cuts off a line cut short as a run stopped as it wrote, which [`read`] left out, so the next line starts a line of
/// its own.
pub(crate) fn cut_to_last_whole(dir: &Path) -> anyhow::Result<()> {
    let path = path(dir);
    let Ok(text) = std::fs::read(&path) else { return Ok(()) };
    let whole = text.iter().rposition(|&byte| byte == b'\n').map_or(0, |at| at + 1);
    if whole < text.len() {
        let file = OpenOptions::new().write(true).open(&path)?;
        file.set_len(u64::try_from(whole)?).with_context(|| format!("couldn't cut {}", path.display()))?;
    }
    Ok(())
}
