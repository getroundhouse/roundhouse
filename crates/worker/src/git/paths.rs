//! Paths as git writes them and reads them: from the top of the worktree, with `/` between folders.

use std::path::{Component, Path, PathBuf};

/// A path git writes, which on Unix is the bytes it has, whether or not they're UTF-8.
#[cfg(unix)]
pub(crate) fn path_from_git(path: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt as _;
    PathBuf::from(std::ffi::OsStr::from_bytes(path))
}

/// A path git writes, which Git for Windows writes as UTF-8, since a Windows path is always Unicode.
#[cfg(windows)]
pub(crate) fn path_from_git(path: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(path).into_owned())
}

/// A path from the top of the worktree as git reads one: its parts with `/` between them, each as the bytes it has on
/// Unix, and as UTF-8 on Windows, as [`path_from_git`] reads them back.
pub(crate) fn path_for_git(path: &Path) -> Vec<u8> {
    let mut written = Vec::new();
    for part in path.components() {
        let Component::Normal(part) = part else { continue };
        if !written.is_empty() {
            written.push(b'/');
        }
        written.extend_from_slice(part.as_encoded_bytes());
    }
    written
}

/// Each path in what git listed with a NUL after each, as `-z` has it write them, such as `git ls-files -z`.
pub(crate) fn nul_separated(listed: &[u8]) -> impl Iterator<Item = &[u8]> {
    listed.split(|byte| *byte == 0).filter(|path| !path.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_given_to_git_reads_back_as_the_same_path() {
        let path = Path::new("src").join("deep").join("a b.rs");

        let written = path_for_git(&path);

        assert_eq!(written, b"src/deep/a b.rs");
        assert_eq!(path_from_git(&written), path);
    }

    #[test]
    fn paths_git_lists_with_nuls_are_each_path_and_nothing_after_the_last() {
        let listed: Vec<&[u8]> = nul_separated(b"a.rs\0src/b c.rs\0").collect();

        assert_eq!(listed, [b"a.rs".as_slice(), b"src/b c.rs"]);
        assert_eq!(nul_separated(b"").count(), 0);
    }
}
