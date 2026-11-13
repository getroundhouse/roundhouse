//! Conformance: scenarios that drive an agent's real CLI through its adapter, and check what the adapter made of it,
//! in Roundhouse's terms. A trial plays a scenario against a scripted model, which is free and the same every time, or
//! the real one, which costs a little. Each scripted trial's capture of what crossed the CLI's stdio is a fixture,
//! which `cargo test` replays through the adapter, so the same checks run without the CLI. What's here is every
//! agent's, and each agent's binding, such as `claude::conformance`, has what's its own. `docs/conformance.md` says
//! what a trial is for, and how to play one.

pub mod capture;
pub mod driver;
pub mod lines;
pub mod scenario;
pub mod scenarios;
#[cfg(unix)]
pub mod stand_in;
pub mod trial;

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `path` in single quotes, for a shell.
pub fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// Through `install`, since writing an executable from this multithreaded process can break running it on Linux: a
/// fork in another thread briefly inherits the open file, and `exec` then fails with "text file busy".
pub fn executable(dir: &Path, name: &str, body: &str) -> io::Result<PathBuf> {
    let source = dir.join(format!("{name}.source"));
    std::fs::write(&source, body)?;
    let path = dir.join(name);
    if !Command::new("install").args(["-m", "755"]).arg(&source).arg(&path).status()?.success() {
        return Err(io::Error::other(format!("install {} failed", path.display())));
    }
    Ok(path)
}
