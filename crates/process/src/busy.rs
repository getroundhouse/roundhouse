//! Running a program that was just written, which Linux won't do while anything has it open for writing, and checking
//! which build it says it is.

use std::io;
use std::time::{Duration, Instant};

/// Far longer than a forked child takes to exec, which is all a busy program waits for.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs a program that was just written, as `run` does, waiting out its being busy. Linux won't run a file while
/// anything has it open for writing, and a fork on another thread while it was being written leaves the child holding
/// it until the child execs.
#[expect(
    clippy::disallowed_methods,
    reason = "the pace a program that's busy is run again at, until whatever holds it open for writing lets go"
)]
pub fn run_once_not_busy<T>(mut run: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match run() {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            ran => return ran,
        }
    }
}

/// Runs `program --version`, as one just written can be, and checks that what it prints is `expected`, as a program
/// that says which build it is that way does. With nothing for its input, nothing of its errors, and no console window
/// on Windows, since an app that has none would flash one up.
pub fn check_version(program: &std::path::Path, expected: &str) -> io::Result<()> {
    let mut command = std::process::Command::new(program);
    command.arg("--version").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    crate::without_window(&mut command);
    let output = run_once_not_busy(|| command.output())?;
    let said = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && said.trim() == expected {
        return Ok(());
    }
    Err(io::Error::other(format!("{} says it's {:?}, not {expected:?}", program.display(), said.trim())))
}
