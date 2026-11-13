use super::super::problem::{Kind, Problem};
use std::path::{Path, PathBuf};
use velopack::locator::VelopackLocatorConfig;

/// Where Velopack finds the app and keeps what it downloads, or `None` to let it choose, which it does on a Mac and on
/// Windows in a folder of this user's. On Linux its choice is `/var/tmp/velopack/Roundhouse/packages`, one folder for
/// every user on the machine, so another user could replace a package between its download and its install, or keep
/// this user from downloading at all. So there it keeps them in `updates/`, in this user's data directory. Velopack
/// would find the `AppImage` through `APPIMAGE`, which the app leaves out of what it starts, so it's told where it is.
#[cfg(target_os = "linux")]
pub(in super::super) fn paths(data_dir: &Path) -> Option<VelopackLocatorConfig> {
    use velopack::locator::{LocationContext, auto_locate_app_manifest};
    let appimage = crate::appimage::path()?.to_path_buf();
    let context = LocationContext::FromSpecifiedRootDir(appimage, Some(data_dir.join("updates")));
    let app = auto_locate_app_manifest(context).ok()?;
    let binaries = app.get_current_bin_dir();
    Some(VelopackLocatorConfig {
        RootAppDir: app.get_appimage_path(),
        UpdateExePath: app.get_update_path(),
        PackagesDir: app.get_packages_dir(),
        // Where Velopack's own search reads it, beside the binaries.
        ManifestPath: binaries.join("sq.version"),
        CurrentBinaryDir: binaries,
        IsPortable: true,
    })
}

/// `None` on a Mac and on Windows, where Velopack chooses a folder of this user's.
#[cfg(not(target_os = "linux"))]
pub(in super::super) fn paths(_data_dir: &Path) -> Option<VelopackLocatorConfig> {
    None
}

/// What Velopack replaces when it updates the app: the `.app` this binary runs from on a Mac.
#[cfg(target_os = "macos")]
pub(in super::super) fn installed(_paths: Option<&VelopackLocatorConfig>) -> Option<PathBuf> {
    let binary = std::env::current_exe().ok()?;
    binary.ancestors().find(|dir| dir.extension().is_some_and(|extension| extension == "app")).map(Path::to_path_buf)
}

/// What Velopack replaces when it updates the app: on Linux, the one file the app runs from, which `paths` found.
#[cfg(target_os = "linux")]
pub(in super::super) fn installed(paths: Option<&VelopackLocatorConfig>) -> Option<PathBuf> {
    paths.map(|paths| paths.RootAppDir.clone())
}

/// Windows installs the app in this user's own directory, so there's nothing to check.
#[cfg(target_os = "windows")]
pub(in super::super) fn installed(_paths: Option<&VelopackLocatorConfig>) -> Option<PathBuf> {
    None
}

/// Why the app can't replace itself where it is, if it can't. On a Mac Velopack replaces the whole bundle, so it has
/// to be able to change it and the folder it's in. On Linux it moves the new file over the old one, which needs only
/// the folder, and where that fails it asks for an administrator's password in a dialog of the system's, through
/// `pkexec`, which the app says it can't update there instead of.
pub(in super::super) fn cant_install(installed: &Path) -> Option<Problem> {
    if installed.components().any(|part| part.as_os_str() == "AppTranslocation") {
        let detail = format!(
            "macOS runs this copy from {}, which can't be changed, because it was opened where it was downloaded. \
             Moved to Applications, it can update.",
            installed.display()
        );
        return Some(Problem { kind: Kind::Translocated, detail });
    }
    let folder = installed.parent()?;
    let changed: &[&Path] = if cfg!(target_os = "linux") { &[folder] } else { &[folder, installed] };
    changed.iter().find(|path| !writable(path)).map(|path| Problem {
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
