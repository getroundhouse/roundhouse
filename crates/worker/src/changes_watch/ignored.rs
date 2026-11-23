//! What git ignores in a watched worktree, as the watch keeps it, so it can drop what the OS says of those paths at
//! once, without asking git about each.

use crate::git::path_from_git;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The paths git ignores, from the top of the worktree: the topmost folders it ignores whole, and files it ignores in
/// folders it doesn't.
#[derive(Debug, Default)]
pub(super) struct Ignored {
    folders: HashSet<PathBuf>,
    files: HashSet<PathBuf>,
}

impl Ignored {
    /// From what `git::ignored::ignored` lists, in which a folder ends in `/`.
    pub(super) fn listed(listed: &[Vec<u8>]) -> Self {
        let mut ignored = Self::default();
        for path in listed {
            match path.strip_suffix(b"/") {
                Some(folder) => ignored.folders.insert(path_from_git(folder)),
                None => ignored.files.insert(path_from_git(path)),
            };
        }
        ignored
    }

    /// Takes in that git ignores the folder `folder` whole, as it found of one made since it listed them.
    pub(super) fn add_folder(&mut self, folder: PathBuf) {
        self.folders.insert(folder);
    }

    /// Whether git ignores `path`, from the top of the worktree: it's an ignored file, or in an ignored folder, or one.
    pub(super) fn covers(&self, path: &Path) -> bool {
        self.files.contains(path) || path.ancestors().any(|folder| self.folders.contains(folder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_in_a_folder_git_ignores_is_ignored_and_one_beside_it_is_not() {
        let ignored = Ignored::listed(&[b"target/".to_vec(), b"pkg/build/".to_vec(), b"a.log".to_vec()]);

        let cases = [
            ("target", true),
            ("target/debug/deep/x", true),
            ("pkg/build/y", true),
            ("a.log", true),
            ("pkg/b.log", false),
            ("pkg/src/a.rs", false),
            ("targets/a", false),
        ];
        for (path, covered) in cases {
            assert_eq!(ignored.covers(Path::new(path)), covered, "{path}");
        }
    }
}
