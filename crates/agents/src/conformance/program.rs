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
