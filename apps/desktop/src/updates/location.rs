//! Whether the app is somewhere it can replace itself, which Velopack needs to update it.

use std::path::{Path, PathBuf};

use super::problem::{Kind, Problem};

/// The `.app` this binary runs from, which Velopack replaces whole. Only a Mac app has one.
pub(super) fn bundle() -> Option<PathBuf> {
    let binary = std::env::current_exe().ok()?;
    binary.ancestors().find(|dir| dir.extension().is_some_and(|extension| extension == "app")).map(Path::to_path_buf)
}

/// Why the app can't replace itself where it is, if it can't. Velopack replaces the whole bundle, so it has to be able
/// to change the bundle and the folder it's in.
pub(super) fn cant_install(bundle: &Path) -> Option<Problem> {
    if bundle.components().any(|part| part.as_os_str() == "AppTranslocation") {
        let detail = format!(
            "macOS runs this copy from {}, which can't be changed, because it was opened where it was downloaded. \
             Moved to Applications, it can update.",
            bundle.display()
        );
        return Some(Problem { kind: Kind::Translocated, detail });
    }
    let folder = bundle.parent()?;
    [folder, bundle].into_iter().find(|path| !writable(path)).map(|path| Problem {
        kind: Kind::ReadOnly,
        detail: format!("This user can't change {}, so Roundhouse can't replace itself there.", path.display()),
    })
}

/// Nothing asks on Windows, where an app has no bundle, so nothing there is said to be in the way.
#[cfg(windows)]
fn writable(_path: &Path) -> bool {
    true
}

#[cfg(unix)]
fn writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt as _;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    // SAFETY: `path` is a NUL-terminated string that outlives the call, which only reads it.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(test)]
mod tests;
