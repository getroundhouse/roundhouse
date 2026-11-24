//! Putting a program where a test runs it, such as a script that stands in for another program, so it can be run as
//! soon as it's there.
//!
//! A test binary runs its tests on several threads, and a child that another thread forks inherits every file this
//! process has open until the child runs its own program. On Linux, running a file that anything holds open for
//! writing fails with "text file busy". So a program a test binary writes itself can fail to run, the more often the
//! busier the machine is, and these have another process write it.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

/// Writes `body` as the program at `path`, replacing any there, which can be run once this returns. It's written beside
/// `path` first, and [`install_program`] puts it in place.
pub fn write_program(path: &Path, body: impl AsRef<[u8]>) -> io::Result<()> {
    let mut source = OsString::from(path.as_os_str());
    source.push(".source");
    fs::write(&source, body)?;
    let installed = install_program(Path::new(&source), path);
    fs::remove_file(&source)?;
    installed
}

/// Copies the program at `source` to `path`, replacing any there, which can be run once this returns. Through
/// `install`, which is a process of its own, so this one never has the copy open, and makes it executable. What was at
/// `path` is removed first, rather than written over, since a process may be running it.
#[cfg(unix)]
pub fn install_program(source: &Path, path: &Path) -> io::Result<()> {
    removed(path)?;
    let status = Command::new("install").args(["-m", "755"]).arg(source).arg(path).status()?;
    if !status.success() {
        return Err(io::Error::other(format!("install {} exited with {status}", path.display())));
    }
    Ok(())
}

/// Copies the program at `source` to `path`, replacing any there, which can be run once this returns. A child Windows
/// starts inherits only what it's handed, so nothing holds the copy once it's written. What was at `path` is removed
/// first, rather than written over, since a process may be running it.
#[cfg(windows)]
pub fn install_program(source: &Path, path: &Path) -> io::Result<()> {
    removed(path)?;
    fs::copy(source, path).map(drop)
}

/// Removes the file at `path`, if there's one.
fn removed(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        Ok(()) | Err(_) => Ok(()),
    }
}

// The program is a shell script, which only a Unix runs.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_program_written_runs_at_once_and_replaces_the_one_before() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("says");

        write_program(&program, "#!/bin/sh\necho first\n").unwrap();
        let first = Command::new(&program).output().unwrap();
        write_program(&program, "#!/bin/sh\necho second\n").unwrap();
        let second = Command::new(&program).output().unwrap();

        assert_eq!(String::from_utf8_lossy(&first.stdout), "first\n");
        assert_eq!(String::from_utf8_lossy(&second.stdout), "second\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "only the program is left");
    }
}
