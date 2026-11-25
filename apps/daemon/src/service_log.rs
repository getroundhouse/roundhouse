//! The log of a hub installed as a service, which its service manager opens for it.

use std::path::Path;

/// Moves this hub's log aside if it's long, and goes on to a new one, as it starts. Whoever opens a log does that for
/// a hub they start, as a client does, but a service manager only opens it. So this does nothing for output that isn't
/// that log, as when a service was set up to keep it elsewhere. A log that can't be begun anew goes on in the one it
/// has, moved aside or not.
#[cfg(unix)]
pub(crate) fn begin_anew_if_long(log: &Path) {
    use std::os::fd::{AsFd as _, AsRawFd as _};

    let (out, err) = (std::io::stdout(), std::io::stderr());
    let outputs = [out.as_fd(), err.as_fd()];
    let to_the_log = outputs.map(|output| is_open_as(log, output));
    if to_the_log == [false, false] {
        return;
    }
    logging::move_aside_if_long(log);
    if std::fs::exists(log).unwrap_or(true) {
        return;
    }
    let Ok(new) = private_files::file_options().create(true).append(true).open(log) else { return };
    for (output, to_the_log) in outputs.into_iter().zip(to_the_log) {
        if to_the_log {
            // SAFETY: dup2(2) takes any descriptors. Both are open, and what's replaced is this process's own output,
            // which nothing else is writing to, since `main` calls this before any other thread exists.
            unsafe { libc::dup2(new.as_raw_fd(), output.as_raw_fd()) };
        }
    }
}

/// Nothing on Windows, where a hub can't be installed as a service yet.
#[cfg(windows)]
pub(crate) fn begin_anew_if_long(_log: &Path) {}

/// Whether the file at `path` is what's open as `output`.
#[cfg(unix)]
fn is_open_as(path: &Path, output: std::os::fd::BorrowedFd<'_>) -> bool {
    use std::os::unix::fs::MetadataExt as _;

    let Ok(open) = output.try_clone_to_owned().and_then(|copy| std::fs::File::from(copy).metadata()) else {
        return false;
    };
    std::fs::metadata(path).is_ok_and(|there| (there.dev(), there.ino()) == (open.dev(), open.ino()))
}
