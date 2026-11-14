use crate::BinaryLookup;
use std::path::PathBuf;

/// `PATH` first, then the places the installers use. A hub started from Finder may have neither
/// `~/.local/bin` nor Homebrew on its `PATH`, and it may not be able to read the login shell's.
pub(crate) fn resolve_binary(name: &str, extra: &[&str], lookup: &BinaryLookup) -> Option<PathBuf> {
    if let Some(path) = &lookup.explicit {
        return Some(path.clone());
    }
    // On Windows that's the program its installer puts in place, and not the script npm installs.
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let on_path = lookup.path.iter().flat_map(std::env::split_paths).map(|dir| dir.join(&file));
    let installed = lookup.home.iter().flat_map(|home| {
        std::iter::once(".local/bin").chain(extra.iter().copied()).map(|dir| home.join(dir).join(&file))
    });
    let system = ["/opt/homebrew/bin", "/usr/local/bin"].map(|dir| PathBuf::from(dir).join(&file));
    on_path.chain(installed).chain(system).find(|path| is_executable(path))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Windows goes by a file's extension, which the names looked for already have.
#[cfg(windows)]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}
